// Live content-block re-render: fetches `GET /sutta_content_block` and swaps
// the children of #ssp_content, then re-runs the node-bound initialization
// (the re-init contract in
// docs/sutta-display-settings-and-multi-column-view.md). The page chrome
// (menus, find bar, settings panel, column bar) lives outside #ssp_content
// and survives the swap; document-level delegated handlers in the inlined
// suttas.js survive too.

import * as h from "./helpers";
import { footnote_bottom_bar } from "./footnote_bottom_bar";
import * as ds from "./display_settings";

export function build_content_block_url(
  uid: string,
  layout: string,
  columns: string[],
  show_references: boolean,
  repeat_pali: string = "off",
): string {
  const API_URL = (globalThis as any).API_URL || "http://localhost:4848";
  // Uids contain "/" — encode each, keep "|" as the separator.
  const cols = columns.map(encodeURIComponent).join("|");
  return `${API_URL}/sutta_content_block?uid=${encodeURIComponent(uid)}`
    + `&layout=${encodeURIComponent(layout)}`
    + `&columns=${cols}`
    + `&show_references=${show_references}`
    + `&repeat_pali=${encodeURIComponent(repeat_pali)}`;
}

/**
 * Re-run whatever is bound to content nodes on page load, after an
 * #ssp_content innerHTML swap:
 * - link handlers (webpack bundle);
 * - the footnote bottom bar's IntersectionObserver;
 * - find-bar highlight state (the old highlight spans were replaced);
 * - the inlined suttas.js per-node bindings (variant/comment marks), via
 *   its window.ssp_rebind_content_handlers() entry point;
 * - the display-settings CSS custom properties (per-column color vars follow
 *   the current column list).
 */
export function reinit_sutta_content(): void {
  const ssp = document.SSP;

  if (ssp && typeof ssp.attach_link_handlers === "function") {
    ssp.attach_link_handlers();
  }

  try {
    if (ssp && ssp.show_bottom_footnotes) {
      footnote_bottom_bar.refresh();
    }
  } catch (error) {
    h.log_error(`reinit: footnote bar refresh failed: ${error}`);
  }

  try {
    // The swapped DOM dropped any find highlights; the manager's recover
    // closure references detached nodes. hide() clears its state.
    if (ssp && ssp.find && typeof ssp.find.hide === "function") {
      ssp.find.hide();
    }
  } catch (error) {
    h.log_error(`reinit: find bar reset failed: ${error}`);
  }

  const rebind = (globalThis as any).ssp_rebind_content_handlers;
  if (typeof rebind === "function") {
    try {
      rebind();
    } catch (error) {
      h.log_error(`reinit: ssp_rebind_content_handlers failed: ${error}`);
    }
  }

  ds.refresh_columns();

  // The column bar listens for this (an event rather than an import, to
  // avoid a module cycle: column_bar.ts imports this module for the fetch).
  document.dispatchEvent(new CustomEvent("ssp-content-swapped"));
}

/**
 * Fetch the content block for the current sutta with the given render
 * parameters and swap it into #ssp_content, preserving the scroll position.
 * On a non-200 response the current content is kept and the error logged.
 * Returns true when the swap happened.
 */
export async function fetch_content_block(
  layout: string,
  columns: string[],
  show_references: boolean,
  repeat_pali: string = "off",
): Promise<boolean> {
  const uid = (globalThis as any).SUTTA_UID;
  const content = document.getElementById("ssp_content");
  if (!uid || !content) {
    h.log_error("fetch_content_block: no SUTTA_UID or #ssp_content on this page");
    return false;
  }

  const url = build_content_block_url(uid, layout, columns, show_references, repeat_pali);

  let response: Response;
  try {
    response = await fetch(url);
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error);
    h.log_error(`fetch_content_block failed: ${msg}`);
    return false;
  }
  if (!response.ok) {
    h.log_error(`fetch_content_block: HTTP ${response.status} for ${url}`);
    return false;
  }

  const html = await response.text();
  const scroll_y = window.scrollY;
  content.innerHTML = html;

  // Keep the injected page state in sync so later fetches (and the settings
  // panel) reproduce the current render parameters. The base column list is
  // updated by the caller when it changes (the column bar knows the labels);
  // the Repeat Pāli arrangement is mirrored here so the per-column color
  // vars and color rows follow what the server actually rendered. Solo keeps
  // the columns untouched: the render shows only the opened sutta, but the
  // state must survive a switch back to Columns/Lines.
  const sd = (globalThis as any).SUTTA_DISPLAY;
  if (sd) {
    sd.layout = layout;
    sd.show_references = show_references;
    sd.repeat_pali = repeat_pali;
    if (layout !== "solo" && Array.isArray(sd.columns)) {
      sd.columns = ds.arrange_display_columns(sd.columns, repeat_pali);
    }
  }

  reinit_sutta_content();
  window.scrollTo(0, scroll_y);
  return true;
}

/**
 * Re-render the content block with the current page state, changing only the
 * render parameters (layout, Repeat Pāli). Used by the display-settings
 * panel's Layout / Repeat Pāli controls. The current column list may carry a
 * previous arrangement's repeated Pāli entries — the server collapses them
 * back to one anchor before applying the requested arrangement.
 */
export function refetch_with_params(layout: string, repeat_pali: string = "off"): Promise<boolean> {
  const sd = (globalThis as any).SUTTA_DISPLAY || {};
  const columns: string[] = Array.isArray(sd.columns)
    ? sd.columns.map((c: any) => c.uid)
    : [];
  const show_references = !!sd.show_references;
  return fetch_content_block(layout, columns, show_references, repeat_pali);
}
