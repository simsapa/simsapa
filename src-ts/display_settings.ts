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
  layout: string; // "solo" | "linebyline" | "sidebyside"
  repeat_pali: string; // "off" | "alternate" | "atend"
  // Reading-measure width as percent of the base sutta_max_width (100 =
  // unchanged); applied as the --width-scale CSS var.
  width_percent: number;
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
    repeat_pali: "off",
    width_percent: 100,
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
let rerender_handler: ((layout: string, repeat_pali: string) => void) | null = null;

// Wired in simsapa.ts init to content_reload.ts's content-block re-render.
// Fired on render-affecting changes (layout, Repeat Pāli).
export function set_rerender_handler(handler: (layout: string, repeat_pali: string) => void): void {
  rerender_handler = handler;
}

function request_rerender(): void {
  if (rerender_handler) {
    rerender_handler(settings.layout, settings.repeat_pali);
  }
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
    repeat_pali: defaults_json.repeat_pali || base.repeat_pali,
    width_percent: defaults_json.width_percent || base.width_percent,
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

  // Reading-measure width: scales the line-by-line body measure and the
  // side-by-side per-column cap (see suttas.sass / _suttacentral.sass).
  root.setProperty("--width-scale", String(s.width_percent / 100));

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

  const gradient = column_bg_gradient(s, columns);
  if (gradient) {
    root.setProperty("--cols-bg-image", gradient);
  } else {
    root.removeProperty("--cols-bg-image");
  }
}

/**
 * Continuous column backgrounds for the side-by-side layout. Painting the
 * per-cell background leaves the template block margins (paragraph gaps)
 * uncolored, so Columns mode instead paints full-height stripes on the
 * `.suttacentral.layout-columns` wrapper: one hard-stop gradient stripe per
 * column, matching the flex geometry (equal-width columns, 1em column-gap),
 * with transparent gaps. The per-cell backgrounds are turned off by CSS in
 * that mode (see _suttacentral.sass). Returns null when the layout is not
 * side-by-side or no column has a background color.
 */
export function column_bg_gradient(
  s: SuttaDisplaySettings,
  columns: SuttaDisplayColumn[],
): string | null {
  if (s.layout !== "sidebyside") {
    return null;
  }
  const n_cols = Math.min(columns.length, MAX_COLOR_COLUMNS);
  if (n_cols < 1) {
    return null;
  }
  const colors = columns.slice(0, n_cols).map((col) => s.author_bg_colors[col.author] || null);
  if (!colors.some((c) => c !== null)) {
    return null;
  }
  // Stripe geometry mirrors `span.segment { display: flex; column-gap: 1em }`
  // with equal flex: 1 1 0 cells.
  const w = `(100% - ${n_cols - 1} * 1em) / ${n_cols}`;
  const stops: string[] = [];
  colors.forEach((color, i) => {
    const c = color || "transparent";
    const start = `calc((${w}) * ${i} + ${i} * 1em)`;
    const end = `calc((${w}) * ${i + 1} + ${i} * 1em)`;
    stops.push(`${c} ${start}`, `${c} ${end}`);
    if (i < n_cols - 1) {
      const next_start = `calc((${w}) * ${i + 1} + ${i + 1} * 1em)`;
      stops.push(`transparent ${end}`, `transparent ${next_start}`);
    }
  });
  return `linear-gradient(to right, ${stops.join(", ")})`;
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
  request_rerender();
  if (scope === "save_default") {
    post_settings();
  }
}

export function set_repeat_pali(repeat_pali: string): void {
  if (settings.repeat_pali === repeat_pali) {
    return;
  }
  settings.repeat_pali = repeat_pali;
  request_rerender();
  if (scope === "save_default") {
    post_settings();
  }
}

/**
 * The Repeat Pāli column arrangement, mirroring the server's
 * `AppData::arrange_repeat_pali` (keep the two in sync): the first Pāli
 * column anchors, translations keep their order. Off = Pāli first, once;
 * alternate = Pāli before each translation; atend = Pāli first and once more
 * as the last column. Idempotent: re-arranging an arranged list collapses
 * the repeated Pāli entries back to one anchor first.
 */
export function arrange_display_columns(
  columns: SuttaDisplayColumn[],
  repeat_pali: string,
): SuttaDisplayColumn[] {
  const pali = columns.find((col) => col.is_pali) || null;
  const translations = columns.filter((col) => !col.is_pali);
  if (!pali) {
    return translations;
  }
  if (translations.length === 0) {
    return [pali];
  }
  if (repeat_pali === "alternate") {
    return translations.flatMap((tr) => [pali, tr]);
  }
  if (repeat_pali === "atend") {
    return [pali, ...translations, pali];
  }
  return [pali, ...translations];
}

export function reset_all(): void {
  const previous_layout = settings.layout;
  const previous_repeat_pali = settings.repeat_pali;
  settings = built_in_defaults();
  sync_controls();
  render_color_rows();
  apply_css_vars();
  if (settings.layout !== previous_layout || settings.repeat_pali !== previous_repeat_pali) {
    request_rerender();
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

/**
 * Highlight the Narrow/Normal/Wide width preset matching the current width
 * percent; no preset is active when the slider sits between preset values.
 */
function sync_width_preset_highlight(): void {
  const panel = panel_el();
  if (!panel) {
    return;
  }
  const presets = panel.querySelector<HTMLElement>(".ds-segmented[data-setting='width-preset']");
  if (presets) {
    set_segmented_active(presets, String(settings.width_percent));
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

  const repeat_seg = panel.querySelector<HTMLElement>(".ds-segmented[data-setting='repeat-pali']");
  if (repeat_seg) {
    set_segmented_active(repeat_seg, settings.repeat_pali);
  }

  const width = panel.querySelector<HTMLInputElement>("input.ds-width");
  if (width) {
    width.value = String(settings.width_percent);
  }
  const width_value = panel.querySelector<HTMLElement>("span.ds-width-value");
  if (width_value) {
    width_value.textContent = `${settings.width_percent}%`;
  }
  sync_width_preset_highlight();

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

// --- Color conversions for the custom picker (hex <-> HSV) ---

/** Parse "#rrggbb" (leading # optional) to [r, g, b] 0-255, or null. */
export function parse_hex_color(text: string): [number, number, number] | null {
  const m = text.trim().match(/^#?([0-9a-fA-F]{6})$/);
  if (!m) {
    return null;
  }
  const n = parseInt(m[1], 16);
  return [(n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff];
}

function rgb_to_hex(r: number, g: number, b: number): string {
  const to2 = (x: number) => Math.round(Math.min(255, Math.max(0, x))).toString(16).padStart(2, "0");
  return `#${to2(r)}${to2(g)}${to2(b)}`;
}

/** [r, g, b] 0-255 -> [h, s, v] with h 0-360, s/v 0-100. */
function rgb_to_hsv(r: number, g: number, b: number): [number, number, number] {
  const rn = r / 255, gn = g / 255, bn = b / 255;
  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const d = max - min;
  let h = 0;
  if (d !== 0) {
    if (max === rn) {
      h = 60 * (((gn - bn) / d) % 6);
    } else if (max === gn) {
      h = 60 * ((bn - rn) / d + 2);
    } else {
      h = 60 * ((rn - gn) / d + 4);
    }
  }
  if (h < 0) {
    h += 360;
  }
  const s = max === 0 ? 0 : (d / max) * 100;
  return [h, s, max * 100];
}

/** [h, s, v] (h 0-360, s/v 0-100) -> [r, g, b] 0-255. */
function hsv_to_rgb(h: number, s: number, v: number): [number, number, number] {
  const sn = s / 100, vn = v / 100;
  const c = vn * sn;
  const hp = ((h % 360) + 360) % 360 / 60;
  const x = c * (1 - Math.abs((hp % 2) - 1));
  let rgb: [number, number, number];
  if (hp < 1) { rgb = [c, x, 0]; }
  else if (hp < 2) { rgb = [x, c, 0]; }
  else if (hp < 3) { rgb = [0, c, x]; }
  else if (hp < 4) { rgb = [0, x, c]; }
  else if (hp < 5) { rgb = [x, 0, c]; }
  else { rgb = [c, 0, x]; }
  const m = vn - c;
  return [(rgb[0] + m) * 255, (rgb[1] + m) * 255, (rgb[2] + m) * 255];
}

function hsv_to_hex(h: number, s: number, v: number): string {
  const [r, g, b] = hsv_to_rgb(h, s, v);
  return rgb_to_hex(r, g, b);
}

/**
 * Build the custom color picker shown under the swatch rows: a 2D
 * saturation/value area, a hue slider and a hex input. Drawn with CSS
 * gradients and pointer events — the native <input type="color"> dialog is
 * unreliable in the embedded WebEngineView / Android WebView. Dragging
 * applies the color live; persistence (POST) happens on release / commit.
 */
function build_custom_picker(
  palette: HTMLElement,
  map: Record<string, string>,
  author: string,
  dot: HTMLElement,
): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "ds-custom";

  const sv_area = document.createElement("div");
  sv_area.className = "ds-sv-area";
  const sv_cursor = document.createElement("div");
  sv_cursor.className = "ds-sv-cursor";
  sv_area.appendChild(sv_cursor);
  wrap.appendChild(sv_area);

  const hue = document.createElement("input");
  hue.type = "range";
  hue.className = "ds-hue";
  hue.min = "0";
  hue.max = "360";
  hue.step = "1";
  hue.title = "Hue";
  wrap.appendChild(hue);

  const hex_row = document.createElement("div");
  hex_row.className = "ds-hex-row";
  const preview = document.createElement("span");
  preview.className = "ds-custom-preview";
  hex_row.appendChild(preview);
  const hex_input = document.createElement("input");
  hex_input.type = "text";
  hex_input.className = "ds-hex";
  hex_input.spellcheck = false;
  hex_input.placeholder = "#rrggbb";
  hex_input.title = "Hex colour, e.g. #fff8e1";
  hex_row.appendChild(hex_input);
  wrap.appendChild(hex_row);

  // Picker state, initialized from the current color when set.
  const initial = parse_hex_color(map[author] || "");
  let [h, s, v] = initial ? rgb_to_hsv(...initial) : [45, 25, 95];

  function sync_ui(): void {
    const hex = hsv_to_hex(h, s, v);
    sv_area.style.background =
      `linear-gradient(to top, #000, rgba(0, 0, 0, 0)), linear-gradient(to right, #fff, hsl(${Math.round(h)}, 100%, 50%))`;
    sv_cursor.style.left = `${s}%`;
    sv_cursor.style.top = `${100 - v}%`;
    hue.value = String(Math.round(h));
    hex_input.value = hex;
    preview.style.backgroundColor = hex;
  }

  function apply_color(persist: boolean): void {
    const hex = hsv_to_hex(h, s, v);
    map[author] = hex;
    set_dot_color(dot, hex);
    // A custom color supersedes any highlighted swatch.
    palette.querySelectorAll(".ds-swatch.selected").forEach((el) => el.classList.remove("selected"));
    if (persist) {
      on_setting_changed();
    } else {
      apply_css_vars();
    }
  }

  let dragging = false;
  function sv_update_from_event(event: PointerEvent): void {
    const rect = sv_area.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) {
      return;
    }
    s = Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width)) * 100;
    v = 100 - Math.min(1, Math.max(0, (event.clientY - rect.top) / rect.height)) * 100;
    sync_ui();
    apply_color(false);
  }
  sv_area.addEventListener("pointerdown", (event) => {
    dragging = true;
    sv_area.setPointerCapture(event.pointerId);
    sv_update_from_event(event);
  });
  sv_area.addEventListener("pointermove", (event) => {
    if (dragging) {
      sv_update_from_event(event);
    }
  });
  sv_area.addEventListener("pointerup", (event) => {
    dragging = false;
    sv_area.releasePointerCapture(event.pointerId);
    apply_color(true);
  });

  hue.addEventListener("input", () => {
    h = Number(hue.value);
    sync_ui();
    apply_color(false);
  });
  hue.addEventListener("change", () => {
    apply_color(true);
  });

  hex_input.addEventListener("change", () => {
    const rgb = parse_hex_color(hex_input.value);
    if (rgb) {
      [h, s, v] = rgb_to_hsv(...rgb);
      sync_ui();
      apply_color(true);
    } else {
      // Invalid input: revert the field to the current picker color.
      sync_ui();
    }
  });
  // Keep Enter in the hex field from bubbling into page-level key handlers.
  hex_input.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      hex_input.dispatchEvent(new Event("change"));
    }
  });

  sync_ui();
  return wrap;
}

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

  palette.appendChild(build_custom_picker(palette, map, author, dot));

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
          case "repeat-pali":
            set_repeat_pali(value);
            break;
          case "width-preset": {
            settings.width_percent = Number(value);
            const width_slider = panel.querySelector<HTMLInputElement>("input.ds-width");
            if (width_slider) {
              width_slider.value = value;
            }
            const width_span = panel.querySelector<HTMLElement>("span.ds-width-value");
            if (width_span) {
              width_span.textContent = `${value}%`;
            }
            on_setting_changed();
            break;
          }
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

  const width_slider = panel.querySelector<HTMLInputElement>("input.ds-width");
  if (width_slider) {
    width_slider.addEventListener("input", () => {
      settings.width_percent = Number(width_slider.value);
      const width_span = panel.querySelector<HTMLElement>("span.ds-width-value");
      if (width_span) {
        width_span.textContent = `${width_slider.value}%`;
      }
      sync_width_preset_highlight();
      on_setting_changed();
    });
  }

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
  if (sd && sd.repeat_pali) {
    settings.repeat_pali = sd.repeat_pali;
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
  rerender_handler = null;
}
