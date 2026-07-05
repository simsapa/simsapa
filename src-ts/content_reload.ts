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
  // panel) reproduce the current render parameters. The column list is
  // adopted from the server's X-SSP-Columns header — the authoritative
  // resolved state (Lines-mode non-segmented drop, Repeat-Pāli arrangement)
  // — before reinit runs, so the per-column color vars and the bar's
  // re-render see it. In Solo the server resolution keeps the full column
  // set (only the renderer shows one text), so adoption is safe there too.
  const sd = (globalThis as any).SUTTA_DISPLAY;
  if (sd) {
    sd.layout = layout;
    sd.show_references = show_references;
    sd.repeat_pali = repeat_pali;
    const adopted = parse_columns_header(response.headers.get("X-SSP-Columns"));
    if (adopted) {
      notice_dropped_columns(columns, adopted, sd.columns);
      sd.columns = adopted;
    } else {
      h.log_error("fetch_content_block: no usable X-SSP-Columns header; keeping the client column state");
    }
  }

  reinit_sutta_content();
  // Restore the page scroll position across the swap. Harmless in the
  // block-fallback (sbs-blocks) view: there the page can't scroll (body is
  // overflow: hidden and each column scrolls independently), so scroll_y is 0
  // and this is a no-op — the per-column scroll resets to the top as intended.
  window.scrollTo(0, scroll_y);
  return true;
}

/** Decode the X-SSP-Columns header (percent-encoded JSON array of
 * `{uid, label, author, is_pali}`); null when absent or unparsable. */
export function parse_columns_header(header: string | null): ds.SuttaDisplayColumn[] | null {
  if (!header) {
    return null;
  }
  try {
    const parsed = JSON.parse(decodeURIComponent(header));
    return Array.isArray(parsed) ? parsed : null;
  } catch (_error) {
    return null;
  }
}

/**
 * PRD FR 8: a Lines-mode request whose columns include a non-segmented text
 * drops that column server-side. When the adopted (resolved) list is missing
 * a requested uid, show a one-line transient notice naming the dropped text
 * (label looked up in the pre-swap column state, falling back to the uid).
 */
function notice_dropped_columns(
  requested_uids: string[],
  adopted: ds.SuttaDisplayColumn[],
  previous_columns: ds.SuttaDisplayColumn[] | undefined,
): void {
  const adopted_uids = new Set(adopted.map((col) => col.uid));
  const dropped = Array.from(new Set(requested_uids.filter((uid) => !adopted_uids.has(uid))));
  if (dropped.length === 0) {
    return;
  }
  const label_of = (uid: string): string => {
    const prev = (previous_columns || []).find((col) => col.uid === uid);
    return prev ? prev.label : uid;
  };
  const names = dropped.map(label_of).join(", ");
  show_transient_notice(`${names} has no segmented text — shown only in the Columns layout`);
}

const NOTICE_TIMEOUT_MS = 6000;

/** Show (or replace) the one-line transient notice above the bottom chrome. */
export function show_transient_notice(message: string): void {
  let notice = document.getElementById("sspDropNotice");
  if (!notice) {
    notice = document.createElement("div");
    notice.id = "sspDropNotice";
    notice.className = "ssp-drop-notice";
    document.body.appendChild(notice);
  }
  notice.textContent = message;
  notice.classList.add("show");
  const el = notice;
  const prev_timer = (el as any)._ssp_notice_timer;
  if (prev_timer) {
    clearTimeout(prev_timer);
  }
  (el as any)._ssp_notice_timer = setTimeout(() => {
    el.classList.remove("show");
  }, NOTICE_TIMEOUT_MS);
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
