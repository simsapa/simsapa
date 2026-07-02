/**
 * Tests for the display-settings panel scope semantics and CSS-var
 * application (src-ts/display_settings.ts).
 */

import * as ds from "./display_settings";

function save_settings_calls(fetch_mock: jest.Mock): any[][] {
  return fetch_mock.mock.calls.filter((call) =>
    String(call[0]).includes("save_sutta_display_settings"));
}

describe("display_settings scope semantics", () => {
  let fetch_mock: jest.Mock;

  beforeEach(() => {
    ds.reset_module_state_for_tests();
    fetch_mock = jest.fn().mockResolvedValue({ ok: true, text: async () => "" });
    (globalThis as any).fetch = fetch_mock;
    (globalThis as any).API_URL = "http://localhost:4848";
    (globalThis as any).SUTTA_DISPLAY = {
      layout: "linebyline",
      columns: [
        { uid: "mn1/en/sujato", label: "sujato", author: "sujato", is_pali: false },
        { uid: "mn1/pli/ms", label: "Pāli", author: "pali", is_pali: true },
      ],
      show_references: false,
      defaults: null,
    };
    document.documentElement.removeAttribute("style");
  });

  test("default scope is save_default and changes POST the settings", () => {
    expect(ds.get_scope()).toBe("save_default");
    ds.on_setting_changed();
    expect(save_settings_calls(fetch_mock).length).toBe(1);
    const body = JSON.parse(save_settings_calls(fetch_mock)[0][1].body);
    expect(body.pali_font.size_percent).toBe(80);
    expect(body.translation_font.family_kind).toBe("serif");
  });

  test("changes in local scope do not POST", () => {
    ds.on_scope_changed("this_view");
    ds.on_setting_changed();
    ds.on_setting_changed();
    expect(save_settings_calls(fetch_mock).length).toBe(0);
  });

  test("switching local -> default immediately POSTs the current state", () => {
    ds.on_scope_changed("this_view");
    ds.get_settings().translation_font.size_percent = 120;
    ds.on_setting_changed();
    expect(save_settings_calls(fetch_mock).length).toBe(0);

    ds.on_scope_changed("save_default");
    const calls = save_settings_calls(fetch_mock);
    expect(calls.length).toBe(1);
    const body = JSON.parse(calls[0][1].body);
    expect(body.translation_font.size_percent).toBe(120);
  });

  test("switching default -> local does not POST", () => {
    ds.on_scope_changed("this_view");
    expect(save_settings_calls(fetch_mock).length).toBe(0);
  });

  test("set_layout triggers the re-render handler and POSTs in default scope", () => {
    const handler = jest.fn();
    ds.set_layout_change_handler(handler);
    ds.set_layout("sidebyside");
    expect(handler).toHaveBeenCalledWith("sidebyside");
    expect(save_settings_calls(fetch_mock).length).toBe(1);
    expect(ds.get_settings().layout).toBe("sidebyside");
  });

  test("set_layout in local scope re-renders but does not POST", () => {
    const handler = jest.fn();
    ds.set_layout_change_handler(handler);
    ds.on_scope_changed("this_view");
    ds.set_layout("sidebyside");
    expect(handler).toHaveBeenCalledWith("sidebyside");
    expect(save_settings_calls(fetch_mock).length).toBe(0);
  });

  test("set_layout with the unchanged layout is a no-op", () => {
    const handler = jest.fn();
    ds.set_layout_change_handler(handler);
    ds.set_layout("linebyline");
    expect(handler).not.toHaveBeenCalled();
    expect(save_settings_calls(fetch_mock).length).toBe(0);
  });

  test("reset_all restores built-in defaults and POSTs in default scope", () => {
    ds.get_settings().pali_font.size_percent = 150;
    ds.reset_all();
    expect(ds.get_settings().pali_font.size_percent).toBe(80);
    expect(save_settings_calls(fetch_mock).length).toBe(1);
  });
});

describe("color rows and swatch palette", () => {
  let fetch_mock: jest.Mock;

  beforeEach(() => {
    ds.reset_module_state_for_tests();
    fetch_mock = jest.fn().mockResolvedValue({ ok: true, text: async () => "" });
    (globalThis as any).fetch = fetch_mock;
    (globalThis as any).SUTTA_DISPLAY = {
      layout: "linebyline",
      columns: [
        { uid: "mn1/en/sujato", label: "sujato", author: "sujato", is_pali: false },
        { uid: "mn1/pli/ms", label: "Pāli", author: "pali", is_pali: true },
      ],
      show_references: false,
      defaults: null,
    };
    document.body.innerHTML = "<div id='dsColorRows'></div>";
    document.documentElement.removeAttribute("style");
  });

  test("renders one row per author with ink/bg dots", () => {
    ds.render_color_rows();
    const rows = document.querySelectorAll(".ds-color-row");
    expect(rows.length).toBe(2);
    expect(rows[0].querySelector(".ds-ink-color")).not.toBeNull();
    expect(rows[0].querySelector(".ds-bg-color")).not.toBeNull();
    expect(rows[0].textContent).toContain("sujato");
  });

  test("clicking a dot opens the palette; a swatch applies and persists", () => {
    ds.render_color_rows();
    const ink_dot = document.querySelector<HTMLButtonElement>(".ds-color-row[data-author='sujato'] .ds-ink-color")!;
    ink_dot.click();

    const palette = document.querySelector<HTMLElement>(".ds-palette")!;
    expect(palette).not.toBeNull();
    expect(palette.dataset.kind).toBe("ink");

    const swatch = palette.querySelectorAll<HTMLButtonElement>(".ds-swatch:not(.ds-swatch-none)")[0];
    swatch.click();

    const color = ds.get_settings().author_ink_colors["sujato"];
    expect(color).toBeTruthy();
    // sujato is column 0; the CSS var follows immediately.
    expect(document.documentElement.style.getPropertyValue("--col-0-ink")).toBe(color);
    // Palette closes after picking; default scope POSTs the settings.
    expect(document.querySelector(".ds-palette")).toBeNull();
    const saves = fetch_mock.mock.calls.filter((c) => String(c[0]).includes("save_sutta_display_settings"));
    expect(saves.length).toBe(1);
  });

  test("the none-swatch clears the stored color", () => {
    ds.get_settings().author_ink_colors["sujato"] = "#1565c0";
    ds.render_color_rows();
    const ink_dot = document.querySelector<HTMLButtonElement>(".ds-color-row[data-author='sujato'] .ds-ink-color")!;
    ink_dot.click();

    document.querySelector<HTMLButtonElement>(".ds-palette .ds-swatch-none")!.click();
    expect(ds.get_settings().author_ink_colors["sujato"]).toBeUndefined();
    expect(document.documentElement.style.getPropertyValue("--col-0-ink")).toBe("");
  });
});

describe("apply_css_vars", () => {
  beforeEach(() => {
    ds.reset_module_state_for_tests();
    (globalThis as any).fetch = jest.fn().mockResolvedValue({ ok: true });
    (globalThis as any).SUTTA_DISPLAY = {
      layout: "sidebyside",
      columns: [
        { uid: "mn1/pli/ms", label: "Pāli", author: "pali", is_pali: true },
        { uid: "mn1/en/sujato", label: "sujato", author: "sujato", is_pali: false },
      ],
      show_references: false,
      defaults: null,
    };
    document.documentElement.removeAttribute("style");
  });

  test("sets font group vars from the settings", () => {
    ds.apply_css_vars();
    const style = document.documentElement.style;
    expect(style.getPropertyValue("--pali-font-size")).toBe("0.8em");
    expect(style.getPropertyValue("--pali-line-height")).toBe("1.5");
    expect(style.getPropertyValue("--tr-font-size")).toBe("1em");
    expect(style.getPropertyValue("--tr-font-family")).toContain("Crimson Pro");
    expect(style.getPropertyValue("--pali-font-family")).toContain("Source Sans");
  });

  test("weight/style vars are only set while Bold/Italic is toggled on", () => {
    ds.apply_css_vars();
    const style = document.documentElement.style;
    // Unset by default: the CSS fallback `inherit` must keep template
    // headings bold.
    expect(style.getPropertyValue("--pali-font-weight")).toBe("");
    expect(style.getPropertyValue("--tr-font-style")).toBe("");

    const s = ds.get_settings();
    s.pali_font.bold = true;
    s.translation_font.italic = true;
    ds.apply_css_vars();
    expect(style.getPropertyValue("--pali-font-weight")).toBe("bold");
    expect(style.getPropertyValue("--tr-font-style")).toBe("italic");

    s.pali_font.bold = false;
    s.translation_font.italic = false;
    ds.apply_css_vars();
    expect(style.getPropertyValue("--pali-font-weight")).toBe("");
    expect(style.getPropertyValue("--tr-font-style")).toBe("");
  });

  test("maps author colors to per-column-index vars", () => {
    const s = ds.get_settings();
    s.author_ink_colors["sujato"] = "#663399";
    s.author_bg_colors["pali"] = "#f0f0f0";
    ds.apply_css_vars();
    const style = document.documentElement.style;
    // pali is column 0, sujato is column 1 in this column list
    expect(style.getPropertyValue("--col-0-bg")).toBe("#f0f0f0");
    expect(style.getPropertyValue("--col-1-ink")).toBe("#663399");
    expect(style.getPropertyValue("--col-0-ink")).toBe("");
    expect(style.getPropertyValue("--col-1-bg")).toBe("");
  });

  test("clears stale column vars when the column order changes", () => {
    const s = ds.get_settings();
    s.author_ink_colors["sujato"] = "#663399";
    ds.apply_css_vars();
    expect(document.documentElement.style.getPropertyValue("--col-1-ink")).toBe("#663399");

    // Swap the columns: sujato is now column 0.
    (globalThis as any).SUTTA_DISPLAY.columns = [
      { uid: "mn1/en/sujato", label: "sujato", author: "sujato", is_pali: false },
      { uid: "mn1/pli/ms", label: "Pāli", author: "pali", is_pali: true },
    ];
    ds.apply_css_vars();
    expect(document.documentElement.style.getPropertyValue("--col-0-ink")).toBe("#663399");
    expect(document.documentElement.style.getPropertyValue("--col-1-ink")).toBe("");
  });
});
