// In-page sutta display settings panel (the bottom-right cogwheel).
//
// Initial state comes from the injected `window.SUTTA_DISPLAY` object
// (effective layout, column list, persisted `defaults`). Typography and color
// changes are applied as CSS custom properties only (no re-render); layout
// changes go through a re-render handler wired to content_reload.ts. See
// tasks/2026-07-01-192905-prd---side-by-side-translation-view.md FR 17–20.

import * as h from "./helpers";

export type Scope = "save_default" | "this_view";
export type FontFamilyKind = "serif" | "sans";

export interface SuttaFontGroup {
  family_kind: FontFamilyKind;
  // Percent of the base sutta font size (100 = 1em).
  size_percent: number;
  // Percent (150 = line-height 1.5).
  line_height_percent: number;
  bold: boolean;
  italic: boolean;
}

export interface SuttaDisplaySettings {
  layout: string; // "linebyline" | "sidebyside"
  pali_font: SuttaFontGroup;
  translation_font: SuttaFontGroup;
  author_ink_colors: Record<string, string>;
  author_bg_colors: Record<string, string>;
}

export interface SuttaDisplayColumn {
  uid: string;
  label: string;
  // Color-map key: the author uid, or "pali" for the Pāli column.
  author: string;
  is_pali: boolean;
}

// Mirrors SuttaDisplayDefaults::default() in backend/src/app_settings.rs and
// the un-overridden stylesheet look (_suttacentral.sass): Pāli cells sans at
// 0.8em, translation cells the serif body font at 1em, line height 1.5.
export function built_in_defaults(): SuttaDisplaySettings {
  return {
    layout: "linebyline",
    pali_font: { family_kind: "sans", size_percent: 80, line_height_percent: 150, bold: false, italic: false },
    translation_font: { family_kind: "serif", size_percent: 100, line_height_percent: 150, bold: false, italic: false },
    author_ink_colors: {},
    author_bg_colors: {},
  };
}

const FONT_STACKS: Record<FontFamilyKind, string> = {
  serif: '"Crimson Pro SSP", serif',
  sans: '"Source Sans 3 SSP", sans-serif',
};

// The sass enumerates per-column color vars for col-0..col-11.
const MAX_COLOR_COLUMNS = 12;

let settings: SuttaDisplaySettings = built_in_defaults();
let scope: Scope = "save_default";
let layout_change_handler: ((layout: string) => void) | null = null;

// Wired in simsapa.ts init to content_reload.ts's content-block re-render.
export function set_layout_change_handler(handler: (layout: string) => void): void {
  layout_change_handler = handler;
}

export function get_settings(): SuttaDisplaySettings {
  return settings;
}

export function get_scope(): Scope {
  return scope;
}

export function current_columns(): SuttaDisplayColumn[] {
  const sd = (globalThis as any).SUTTA_DISPLAY;
  if (sd && Array.isArray(sd.columns)) {
    return sd.columns as SuttaDisplayColumn[];
  }
  return [];
}

function merged_settings(defaults_json: any): SuttaDisplaySettings {
  const base = built_in_defaults();
  if (!defaults_json) {
    return base;
  }
  return {
    layout: defaults_json.layout || base.layout,
    pali_font: { ...base.pali_font, ...(defaults_json.pali_font || {}) },
    translation_font: { ...base.translation_font, ...(defaults_json.translation_font || {}) },
    author_ink_colors: { ...(defaults_json.author_ink_colors || {}) },
    author_bg_colors: { ...(defaults_json.author_bg_colors || {}) },
  };
}

/**
 * Apply the typography and color settings as CSS custom properties on
 * document.documentElement. Colors are stored keyed by author and mapped to
 * per-column-index vars from the current column list.
 */
export function apply_css_vars(
  s: SuttaDisplaySettings = settings,
  columns: SuttaDisplayColumn[] = current_columns(),
): void {
  const root = document.documentElement.style;

  root.setProperty("--pali-font-family", FONT_STACKS[s.pali_font.family_kind]);
  root.setProperty("--pali-font-size", `${s.pali_font.size_percent / 100}em`);
  root.setProperty("--pali-line-height", `${s.pali_font.line_height_percent / 100}`);

  root.setProperty("--tr-font-family", FONT_STACKS[s.translation_font.family_kind]);
  root.setProperty("--tr-font-size", `${s.translation_font.size_percent / 100}em`);
  root.setProperty("--tr-line-height", `${s.translation_font.line_height_percent / 100}`);

  // Weight/style are only set while toggled on: the CSS fallback is
  // `inherit`, which keeps template headings (h1 etc. wrapping a segment)
  // bold. Setting "normal" unconditionally would override them.
  const weight_style_vars: Array<[string, SuttaFontGroup]> = [
    ["pali", s.pali_font],
    ["tr", s.translation_font],
  ];
  for (const [prefix, fg] of weight_style_vars) {
    if (fg.bold) {
      root.setProperty(`--${prefix}-font-weight`, "bold");
    } else {
      root.removeProperty(`--${prefix}-font-weight`);
    }
    if (fg.italic) {
      root.setProperty(`--${prefix}-font-style`, "italic");
    } else {
      root.removeProperty(`--${prefix}-font-style`);
    }
  }

  for (let n = 0; n < MAX_COLOR_COLUMNS; n++) {
    root.removeProperty(`--col-${n}-ink`);
    root.removeProperty(`--col-${n}-bg`);
  }
  columns.forEach((col, n) => {
    if (n >= MAX_COLOR_COLUMNS) {
      return;
    }
    const ink = s.author_ink_colors[col.author];
    if (ink) {
      root.setProperty(`--col-${n}-ink`, ink);
    }
    const bg = s.author_bg_colors[col.author];
    if (bg) {
      root.setProperty(`--col-${n}-bg`, bg);
    }
  });
}

async function post_settings(): Promise<void> {
  const API_URL = (globalThis as any).API_URL || "http://localhost:4848";
  try {
    const response = await fetch(`${API_URL}/save_sutta_display_settings`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(settings),
    });
    if (!response.ok) {
      h.log_error(`save_sutta_display_settings failed: ${response.status}`);
    }
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error);
    h.log_error(`save_sutta_display_settings error: ${msg}`);
  }
}

/**
 * A setting changed in the panel: apply it locally, and persist when the
 * scope is "Save as default".
 */
export function on_setting_changed(): void {
  apply_css_vars();
  if (scope === "save_default") {
    post_settings();
  }
}

/**
 * Scope switch. Switching from "This view only" to "Save as default"
 * immediately persists the current in-page settings state, even with no
 * further changes.
 */
export function on_scope_changed(new_scope: Scope): void {
  const was_local = scope === "this_view";
  scope = new_scope;
  if (was_local && new_scope === "save_default") {
    post_settings();
  }
}

export function set_layout(layout: string): void {
  if (settings.layout === layout) {
    return;
  }
  settings.layout = layout;
  if (layout_change_handler) {
    layout_change_handler(layout);
  }
  if (scope === "save_default") {
    post_settings();
  }
}

export function reset_all(): void {
  const previous_layout = settings.layout;
  settings = built_in_defaults();
  sync_controls();
  render_color_rows();
  apply_css_vars();
  if (settings.layout !== previous_layout && layout_change_handler) {
    layout_change_handler(settings.layout);
  }
  if (scope === "save_default") {
    post_settings();
  }
}

function panel_el(): HTMLElement | null {
  return document.getElementById("displaySettingsPanel");
}

function font_group(group: string): SuttaFontGroup {
  return group === "pali" ? settings.pali_font : settings.translation_font;
}

/** Highlight the segmented-control button matching `value` within one group. */
function set_segmented_active(segmented: HTMLElement, value: string): void {
  segmented.querySelectorAll<HTMLButtonElement>(".ds-seg-btn").forEach((btn) => {
    btn.classList.toggle("active", btn.dataset.value === value);
  });
}

/**
 * Highlight the Compact/Normal/Relaxed preset matching the group's current
 * line-height percent; no preset is active when the slider sits between
 * preset values.
 */
function sync_lh_preset_highlight(group: string): void {
  const panel = panel_el();
  if (!panel) {
    return;
  }
  const presets = panel.querySelector<HTMLElement>(`.ds-segmented[data-setting='line-height-preset'][data-group='${group}']`);
  if (presets) {
    set_segmented_active(presets, String(font_group(group).line_height_percent));
  }
}

/** Set the panel's controls to the current settings state. */
function sync_controls(): void {
  const panel = panel_el();
  if (!panel) {
    return;
  }

  const scope_seg = panel.querySelector<HTMLElement>(".ds-segmented[data-setting='scope']");
  if (scope_seg) {
    set_segmented_active(scope_seg, scope);
  }

  const layout_seg = panel.querySelector<HTMLElement>(".ds-segmented[data-setting='layout']");
  if (layout_seg) {
    set_segmented_active(layout_seg, settings.layout);
  }

  for (const group of ["pali", "translation"]) {
    const fg = font_group(group);
    const family_seg = panel.querySelector<HTMLElement>(`.ds-segmented[data-setting='font-family'][data-group='${group}']`);
    if (family_seg) {
      set_segmented_active(family_seg, fg.family_kind);
    }
    const size = panel.querySelector<HTMLInputElement>(`input.ds-font-size[data-group='${group}']`);
    if (size) {
      size.value = String(fg.size_percent);
    }
    const size_value = panel.querySelector<HTMLElement>(`span.ds-font-size-value[data-group='${group}']`);
    if (size_value) {
      size_value.textContent = `${fg.size_percent}%`;
    }
    const line_height = panel.querySelector<HTMLInputElement>(`input.ds-line-height[data-group='${group}']`);
    if (line_height) {
      line_height.value = String(fg.line_height_percent);
    }
    const line_height_value = panel.querySelector<HTMLElement>(`span.ds-line-height-value[data-group='${group}']`);
    if (line_height_value) {
      line_height_value.textContent = `${fg.line_height_percent}%`;
    }
    sync_lh_preset_highlight(group);

    const bold_btn = panel.querySelector<HTMLButtonElement>(`.ds-style-toggles[data-group='${group}'] .ds-toggle[data-toggle='bold']`);
    if (bold_btn) {
      bold_btn.classList.toggle("active", fg.bold);
    }
    const italic_btn = panel.querySelector<HTMLButtonElement>(`.ds-style-toggles[data-group='${group}'] .ds-toggle[data-toggle='italic']`);
    if (italic_btn) {
      italic_btn.classList.toggle("active", fg.italic);
    }
  }
}

/**
 * Render one ink + background color row per currently shown column into
 * #dsColorRows. Colors are stored keyed by the column's author.
 */
export function render_color_rows(): void {
  const container = document.getElementById("dsColorRows");
  if (!container) {
    return;
  }
  container.innerHTML = "";
  close_palette();

  // One row per author (the same author may not appear twice, but the Pāli
  // column shares the map key "pali" — keep rows unique by author key).
  const seen = new Set<string>();
  for (const col of current_columns()) {
    if (seen.has(col.author)) {
      continue;
    }
    seen.add(col.author);

    // Row layout follows the jhana.info "translator inks" pattern: a color
    // dot per text, its name, then a Reset link. Ours has two dots — ink
    // (text color) and column background. The dots open an inline swatch
    // palette (see open_palette) rather than <input type="color">: the
    // native color dialog is unreliable in an embedded QML WebEngineView.
    const row = document.createElement("div");
    row.className = "ds-color-row";
    row.dataset.author = col.author;

    const ink = document.createElement("button");
    ink.type = "button";
    ink.className = "ds-color-dot ds-ink-color";
    ink.title = "Text colour";
    set_dot_color(ink, settings.author_ink_colors[col.author]);
    ink.addEventListener("click", () => {
      open_palette(row, col.author, "ink", ink);
    });
    row.appendChild(ink);

    const bg = document.createElement("button");
    bg.type = "button";
    bg.className = "ds-color-dot ds-bg-color";
    bg.title = "Column background colour";
    set_dot_color(bg, settings.author_bg_colors[col.author]);
    bg.addEventListener("click", () => {
      open_palette(row, col.author, "bg", bg);
    });
    row.appendChild(bg);

    const label = document.createElement("span");
    label.className = "ds-color-label";
    label.textContent = col.label;
    row.appendChild(label);

    const clear = document.createElement("button");
    clear.type = "button";
    clear.className = "ds-color-clear";
    clear.title = "Clear colours for this text";
    clear.textContent = "Reset";
    clear.addEventListener("click", () => {
      delete settings.author_ink_colors[col.author];
      delete settings.author_bg_colors[col.author];
      set_dot_color(ink, undefined);
      set_dot_color(bg, undefined);
      close_palette();
      on_setting_changed();
    });
    row.appendChild(clear);

    container.appendChild(row);
  }
}

// Swatch palettes for the inline color picker. Ink rows offer dark colors
// (light theme) and light variants (dark theme); backgrounds offer pale
// tints plus dark tints for the dark theme.
const PALETTE_INKS: string[] = [
  "#00695c", "#1565c0", "#e65100", "#6a1b9a", "#b71c1c", "#2e7d32",
  "#5d4037", "#ad1457", "#283593", "#827717", "#455a64", "#000000",
  "#80cbc4", "#90caf9", "#ffb74d", "#ce93d8", "#ef9a9a", "#a5d6a7",
];
const PALETTE_BGS: string[] = [
  "#fff8e1", "#e3f2fd", "#fce4ec", "#e8f5e9", "#f3e5f5", "#fbe9e7",
  "#e0f2f1", "#f9fbe7", "#ede7f6", "#eceff1", "#fffde7", "#efebe9",
  "#263238", "#2d3a2e", "#3a2e39", "#33302a", "#2a3340", "#402a2a",
];

let open_palette_el: HTMLElement | null = null;

function set_dot_color(dot: HTMLElement, color: string | undefined): void {
  dot.style.backgroundColor = color || "transparent";
  dot.classList.toggle("is-set", !!color);
}

function close_palette(): void {
  if (open_palette_el) {
    open_palette_el.remove();
    open_palette_el = null;
  }
}

/**
 * Open the inline swatch palette for one author's ink or background color,
 * inserted directly under its row. Clicking the same dot again closes it.
 */
function open_palette(row: HTMLElement, author: string, kind: "ink" | "bg", dot: HTMLElement): void {
  const was_open = open_palette_el !== null
    && open_palette_el.dataset.author === author
    && open_palette_el.dataset.kind === kind;
  close_palette();
  if (was_open) {
    return;
  }

  const map = kind === "ink" ? settings.author_ink_colors : settings.author_bg_colors;
  const palette = document.createElement("div");
  palette.className = "ds-palette";
  palette.dataset.author = author;
  palette.dataset.kind = kind;

  const none = document.createElement("button");
  none.type = "button";
  none.className = "ds-swatch ds-swatch-none";
  none.title = "Default (no colour)";
  none.textContent = "×";
  none.addEventListener("click", () => {
    delete map[author];
    set_dot_color(dot, undefined);
    close_palette();
    on_setting_changed();
  });
  palette.appendChild(none);

  const colors = kind === "ink" ? PALETTE_INKS : PALETTE_BGS;
  for (const color of colors) {
    const swatch = document.createElement("button");
    swatch.type = "button";
    swatch.className = "ds-swatch";
    swatch.title = color;
    swatch.style.backgroundColor = color;
    swatch.classList.toggle("selected", map[author] === color);
    swatch.addEventListener("click", () => {
      map[author] = color;
      set_dot_color(dot, color);
      close_palette();
      on_setting_changed();
    });
    palette.appendChild(swatch);
  }

  row.insertAdjacentElement("afterend", palette);
  open_palette_el = palette;
}

/**
 * Called after a content-block swap or column-set change: the column list in
 * window.SUTTA_DISPLAY has been updated, so the color rows and the
 * column-index color vars must follow.
 */
export function refresh_columns(): void {
  render_color_rows();
  apply_css_vars();
}

function wire_panel(): void {
  const button = document.getElementById("displaySettingsButton");
  const panel = panel_el();
  if (!button || !panel) {
    return;
  }

  button.addEventListener("click", () => {
    panel.classList.toggle("show");
  });

  // Click-away closes an open swatch palette (clicks on dots and swatches
  // are handled by their own listeners).
  document.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    if (open_palette_el && target && !target.closest(".ds-palette, .ds-color-dot")) {
      close_palette();
    }
  });

  // Exclusive segmented controls. The style-toggle groups (`.ds-style-toggles`)
  // have no data-setting and are wired separately below — their buttons
  // toggle independently.
  panel.querySelectorAll<HTMLElement>(".ds-segmented[data-setting]").forEach((segmented) => {
    segmented.querySelectorAll<HTMLButtonElement>(".ds-seg-btn").forEach((btn) => {
      btn.addEventListener("click", () => {
        const value = btn.dataset.value || "";
        const group = segmented.dataset.group || "";
        set_segmented_active(segmented, value);
        switch (segmented.dataset.setting) {
          case "scope":
            on_scope_changed(value as Scope);
            break;
          case "layout":
            set_layout(value);
            break;
          case "font-family":
            font_group(group).family_kind = value as FontFamilyKind;
            on_setting_changed();
            break;
          case "line-height-preset": {
            font_group(group).line_height_percent = Number(value);
            const slider = panel.querySelector<HTMLInputElement>(`input.ds-line-height[data-group='${group}']`);
            if (slider) {
              slider.value = value;
            }
            const value_span = panel.querySelector<HTMLElement>(`span.ds-line-height-value[data-group='${group}']`);
            if (value_span) {
              value_span.textContent = `${value}%`;
            }
            on_setting_changed();
            break;
          }
        }
      });
    });
  });

  // Bold / Italic: independent on/off toggles per font group.
  panel.querySelectorAll<HTMLButtonElement>(".ds-style-toggles .ds-toggle").forEach((btn) => {
    btn.addEventListener("click", () => {
      const fg = font_group(btn.dataset.group || "");
      if (btn.dataset.toggle === "bold") {
        fg.bold = !fg.bold;
        btn.classList.toggle("active", fg.bold);
      } else if (btn.dataset.toggle === "italic") {
        fg.italic = !fg.italic;
        btn.classList.toggle("active", fg.italic);
      }
      on_setting_changed();
    });
  });

  panel.querySelectorAll<HTMLInputElement>("input.ds-font-size").forEach((range) => {
    range.addEventListener("input", () => {
      const group = range.dataset.group || "";
      font_group(group).size_percent = Number(range.value);
      const value = panel.querySelector<HTMLElement>(`span.ds-font-size-value[data-group='${group}']`);
      if (value) {
        value.textContent = `${range.value}%`;
      }
      on_setting_changed();
    });
  });

  panel.querySelectorAll<HTMLInputElement>("input.ds-line-height").forEach((range) => {
    range.addEventListener("input", () => {
      const group = range.dataset.group || "";
      font_group(group).line_height_percent = Number(range.value);
      const value = panel.querySelector<HTMLElement>(`span.ds-line-height-value[data-group='${group}']`);
      if (value) {
        value.textContent = `${range.value}%`;
      }
      sync_lh_preset_highlight(group);
      on_setting_changed();
    });
  });

  const reset = document.getElementById("dsResetAll");
  if (reset) {
    reset.addEventListener("click", reset_all);
  }
}

/**
 * Initialize the display-settings panel. No-op on pages without the cogwheel
 * chrome (dictionary, book and blank pages).
 */
export function init_display_settings(): void {
  if (!document.getElementById("displaySettingsButton") || !panel_el()) {
    return;
  }

  const sd = (globalThis as any).SUTTA_DISPLAY;
  settings = merged_settings(sd ? sd.defaults : null);
  if (sd && sd.layout) {
    // The page may have been rendered with a GET-param layout override;
    // the panel reflects the effective layout.
    settings.layout = sd.layout;
  }
  scope = "save_default";

  wire_panel();
  sync_controls();
  render_color_rows();
  apply_css_vars();
}

// Test-only state reset (module state persists between jest test cases).
export function reset_module_state_for_tests(): void {
  settings = built_in_defaults();
  scope = "save_default";
  layout_change_handler = null;
}
