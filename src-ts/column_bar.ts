// Bottom column bar: one text dropdown per displayed column, "×" to remove,
// "+" to append. Dropdown options come from GET /translations_for_sutta; the
// current column set lives in window.SUTTA_DISPLAY.columns (the single source
// of truth, shared with display_settings.ts — the panel's color rows follow
// column changes through reinit_sutta_content). Any change re-renders the
// content block via content_reload.fetch_content_block(). See
// docs/sutta-display-settings-and-multi-column-view.md and the PRD FR 21-24.

import * as h from "./helpers";
import * as ds from "./display_settings";
import * as cr from "./content_reload";

export interface TranslationOption {
  uid: string;
  // Color-map / display key: the author uid, or "pali" for a Pāli source
  // (mirrors sutta_display_js in backend/src/app_data.rs).
  author: string;
  language: string;
  is_pali: boolean;
  // false → no segmented (Bilara) content: cannot be interleaved in the
  // Lines layout, so the dropdowns disable it there.
  has_content_json: boolean;
  // Hover title: "MN 1 The Root of All Things".
  title: string;
}

// The /translations_for_sutta options, fetched once at init. The route
// excludes the opened sutta itself; ensure_current_columns_present() fills
// the gap from the injected column state.
let options: TranslationOption[] = [];

export function get_options(): TranslationOption[] {
  return options;
}

function sutta_display(): any {
  return (globalThis as any).SUTTA_DISPLAY;
}

/**
 * The bar's working column set: the injected/arranged column list collapsed
 * back to one Pāli anchor (the Repeat Pāli arrangement repeats the Pāli
 * entry; the bar edits the base set and the server re-applies the
 * arrangement).
 */
export function base_columns(): ds.SuttaDisplayColumn[] {
  return ds.arrange_display_columns(ds.current_columns(), "off");
}

/** Map a /translations_for_sutta entry to a TranslationOption. */
export function to_option(entry: any): TranslationOption {
  const is_pali = entry.language === "pli";
  return {
    uid: entry.item_uid,
    author: is_pali ? "pali" : (entry.author || entry.language),
    language: entry.language || "",
    is_pali,
    has_content_json: !!entry.has_content_json,
    title: `${entry.sutta_ref || ""} ${entry.sutta_title || ""}`.trim(),
  };
}

/**
 * The dropdown text for an option: "author (language)", or for a Pāli source
 * "Pāli (edition)" — several Pāli editions (ms, cst) can be in the list, so
 * the uid's source part distinguishes them.
 */
export function option_text(opt: TranslationOption): string {
  if (opt.is_pali) {
    const edition = opt.uid.split("/")[2] || "";
    return edition ? `Pāli (${edition})` : "Pāli";
  }
  return opt.language ? `${opt.author} (${opt.language})` : opt.author;
}

/** The column label shown in the settings panel's color rows ("Pāli" / author). */
function option_column_label(opt: TranslationOption): string {
  return opt.is_pali ? "Pāli" : opt.author;
}

/**
 * The option set can miss currently displayed columns (the route excludes
 * the opened sutta; a GET-param column may be anything): synthesize entries
 * for them from the injected column state so every dropdown has its current
 * value. A synthesized entry is assumed segmented — if it is displayed in
 * Lines mode it must be, and otherwise the flag only affects its disabled
 * state in Lines mode, where the server would drop it anyway.
 */
export function ensure_current_columns_present(): void {
  for (const col of base_columns()) {
    if (!options.some((opt) => opt.uid === col.uid)) {
      options.push({
        uid: col.uid,
        author: col.author,
        language: col.uid.split("/")[1] || "",
        is_pali: col.is_pali,
        has_content_json: true,
        title: "",
      });
    }
  }
}

/**
 * The "+" button's suggestion: the Pāli text if not shown, else the first
 * unshown translation in list order. Only options the dropdowns would allow
 * are suggested (option_disabled_reason is the shared rule): in the Lines
 * layout non-segmented texts are skipped — the server would silently drop
 * them (PRD FR 8). Null when nothing selectable remains (the button is
 * disabled then, FR 23).
 */
export function next_unshown(opts: TranslationOption[], shown_uids: string[], layout: string): TranslationOption | null {
  const selectable = (opt: TranslationOption) =>
    option_disabled_reason(opt, layout, shown_uids, "") === null;
  const pali = opts.find((opt) => opt.is_pali && selectable(opt));
  if (pali) {
    return pali;
  }
  return opts.find((opt) => !opt.is_pali && selectable(opt)) || null;
}

export const LINES_DISABLED_TITLE = "No segmented text — available in the Columns layout";

/**
 * Whether a dropdown option is selectable: texts shown in another column
 * are disabled (a column set has no duplicates), and in Lines mode
 * non-segmented texts are disabled with an explanatory title.
 * Returns null when selectable, else the reason for the title attribute.
 */
export function option_disabled_reason(
  opt: TranslationOption,
  layout: string,
  shown_uids: string[],
  current_uid: string,
): string | null {
  if (layout === "linebyline" && !opt.has_content_json) {
    return LINES_DISABLED_TITLE;
  }
  if (opt.uid !== current_uid && shown_uids.includes(opt.uid)) {
    return "Already displayed";
  }
  return null;
}

/**
 * Apply a new base column set: update the shared SUTTA_DISPLAY.columns state,
 * re-render the content block, and revert the state if the fetch fails.
 */
async function apply_columns(new_columns: ds.SuttaDisplayColumn[]): Promise<void> {
  const sd = sutta_display();
  if (!sd) {
    return;
  }
  const previous = sd.columns;
  sd.columns = new_columns;
  const ok = await cr.fetch_content_block(
    sd.layout,
    new_columns.map((col) => col.uid),
    !!sd.show_references,
    sd.repeat_pali || "off",
  );
  if (!ok) {
    sd.columns = previous;
  }
  // On success the swap's reinit dispatched ssp-content-swapped, which
  // already re-rendered the bar; on failure this restores the previous
  // dropdown state.
  render_bar();
}

function option_to_column(opt: TranslationOption): ds.SuttaDisplayColumn {
  return {
    uid: opt.uid,
    label: option_column_label(opt),
    author: opt.author,
    is_pali: opt.is_pali,
  };
}

// The one open dropdown menu (or null). A native <select> popup opens
// downward and is clipped at the window edge in the embedded WebEngineView
// (it does not flip up), so the bar uses a custom menu opening upward.
let open_menu_el: HTMLElement | null = null;

function close_menu(): void {
  if (open_menu_el) {
    open_menu_el.classList.remove("show");
    open_menu_el = null;
  }
}

function make_dropdown(columns: ds.SuttaDisplayColumn[], index: number, layout: string): HTMLElement {
  const shown_uids = columns.map((col) => col.uid);
  const current_uid = columns[index].uid;
  const current = options.find((opt) => opt.uid === current_uid);

  const wrap = document.createElement("span");
  wrap.className = "column-bar-dropdown";

  const button = document.createElement("button");
  button.type = "button";
  button.className = "column-bar-select";
  button.textContent = current ? option_text(current) : current_uid;
  if (current && current.title) {
    button.title = current.title;
  }
  wrap.appendChild(button);

  const menu = document.createElement("div");
  menu.className = "column-bar-menu";
  for (const opt of options) {
    const el = document.createElement("button");
    el.type = "button";
    el.className = "column-bar-option";
    el.dataset.uid = opt.uid;
    el.textContent = option_text(opt);
    if (opt.title) {
      el.title = opt.title;
    }
    if (opt.uid === current_uid) {
      el.classList.add("selected");
    }
    const reason = option_disabled_reason(opt, layout, shown_uids, current_uid);
    if (reason) {
      el.disabled = true;
      el.title = opt.title ? `${opt.title} — ${reason}` : reason;
    }
    el.addEventListener("click", () => {
      close_menu();
      if (opt.uid === current_uid) {
        return;
      }
      const new_columns = columns.slice();
      new_columns[index] = option_to_column(opt);
      apply_columns(new_columns);
    });
    menu.appendChild(el);
  }
  wrap.appendChild(menu);

  button.addEventListener("click", () => {
    const was_open = open_menu_el === menu;
    close_menu();
    if (!was_open) {
      menu.classList.add("show");
      open_menu_el = menu;
    }
  });

  return wrap;
}

/**
 * (Re-)render the bar from the current SUTTA_DISPLAY state. Runs at init and
 * after every content swap (the layout may have changed: Lines-mode option
 * disabling and the Solo hide follow it).
 */
export function render_bar(): void {
  const bar = document.getElementById("columnBar");
  const items = document.getElementById("columnBarItems");
  const add = document.getElementById("columnBarAdd") as HTMLButtonElement | null;
  const sd = sutta_display();
  if (!bar || !items || !add || !sd) {
    return;
  }

  // In Solo layout the render shows only the opened sutta; the column state
  // is retained for switching back, but the bar is hidden.
  const layout = sd.layout || "linebyline";
  bar.classList.toggle("show", layout !== "solo");

  ensure_current_columns_present();
  const columns = base_columns();
  const shown_uids = columns.map((col) => col.uid);

  // The dropdowns mirror the column geometry: the cols-N class picks up the
  // same centered max-width cap as the layout-columns wrapper (see
  // _display_settings.scss).
  items.className = `column-bar-items cols-${columns.length}`;
  items.innerHTML = "";
  open_menu_el = null;
  columns.forEach((col, index) => {
    const item = document.createElement("span");
    item.className = "column-bar-item";
    item.appendChild(make_dropdown(columns, index, layout));

    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "column-bar-remove";
    remove.textContent = "×";
    remove.title = "Remove this column";
    // Minimum one column (FR 23).
    remove.disabled = columns.length <= 1;
    remove.addEventListener("click", () => {
      const new_columns = columns.slice();
      new_columns.splice(index, 1);
      apply_columns(new_columns);
    });
    item.appendChild(remove);

    items.appendChild(item);
  });

  const suggestion = next_unshown(options, shown_uids, layout);
  add.disabled = suggestion === null;
  add.title = suggestion === null ? "No more texts can be added" : "Add a column";
}

async function fetch_options(): Promise<void> {
  const API_URL = (globalThis as any).API_URL || "http://localhost:4848";
  const uid = (globalThis as any).SUTTA_UID;
  if (!uid) {
    return;
  }
  try {
    const response = await fetch(`${API_URL}/translations_for_sutta?uid=${encodeURIComponent(uid)}`);
    if (!response.ok) {
      h.log_error(`translations_for_sutta failed: HTTP ${response.status}`);
      return;
    }
    const entries = await response.json();
    if (Array.isArray(entries)) {
      options = entries.map(to_option);
    }
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error);
    h.log_error(`translations_for_sutta error: ${msg}`);
  }
}

/**
 * Initialize the column bar. No-op on pages without the sutta display chrome
 * (dictionary, book and blank pages).
 */
export async function init_column_bar(): Promise<void> {
  const bar = document.getElementById("columnBar");
  const add = document.getElementById("columnBarAdd") as HTMLButtonElement | null;
  if (!bar || !add) {
    return;
  }

  add.addEventListener("click", () => {
    const columns = base_columns();
    const sd = sutta_display();
    const layout = (sd && sd.layout) || "linebyline";
    const suggestion = next_unshown(options, columns.map((col) => col.uid), layout);
    if (!suggestion) {
      return;
    }
    apply_columns([...columns, option_to_column(suggestion)]);
  });

  // Re-render after every content-block swap: the settings panel's Layout /
  // Repeat Pāli changes go through the same swap and change the bar's
  // disable rules and visibility.
  document.addEventListener("ssp-content-swapped", () => {
    render_bar();
  });

  // Click-away closes an open dropdown menu (clicks on the dropdown button
  // toggle it themselves; option clicks close before applying).
  document.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    if (open_menu_el && target && !target.closest(".column-bar-dropdown")) {
      close_menu();
    }
  });

  await fetch_options();
  render_bar();
}

// Test-only state reset (module state persists between jest test cases).
export function reset_module_state_for_tests(): void {
  options = [];
  open_menu_el = null;
}
