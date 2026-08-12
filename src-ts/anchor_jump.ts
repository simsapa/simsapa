/**
 * Paragraph jumps from a segment id (the Topic Index's "dn33:1.11.0").
 *
 * The QML webview wrappers call `window.ssp_jump_to_segment(id)` after the page
 * loads. The logic lives here rather than in the wrappers' JS string literals
 * so that it exists once instead of twice (desktop + mobile) and is reachable
 * from Jest.
 *
 * See docs/sutta-display-settings-and-multi-column-view.md for the anchor-jump
 * section: the deliberate stopping rule of the candidate walk, the two notice
 * forms, and why a notice may never be injected into `span.segment`.
 */

const NOTICE_CLASS = "ssp-anchor-notice";
const HIGHLIGHT_CLASS = "ssp-anchor-highlight";

// How long the landing highlight stays before it is removed again. The fade
// itself is a CSS animation; this only cleans the class up so a later jump to
// the same paragraph can re-trigger it.
const HIGHLIGHT_MS = 2500;

/**
 * The ids to try, in order, for a requested segment id.
 *
 * 1. the requested id itself;
 * 2. the last numeric component decremented down to 0 — `1.7.9.10` gives
 *    `1.7.9.9` … `1.7.9.0`;
 * 3. the parent, exactly once — `1.7.9`.
 *
 * Then stop. Never `1.7.8`, never `1.7`: a sibling of the parent may be an
 * entirely different chapter of the sutta, and landing there silently is worse
 * than landing at the top.
 *
 * A last component that is not numeric skips step 2 (no decrements to make).
 * Step 3 does not fire against the current data — Bilara emits headings as
 * `x.y.z.0`, never as the bare parent — but it costs two lines and is covered
 * by the unit tests.
 */
export function candidate_ids(requested: string): string[] {
    const ids: string[] = [requested];

    // A segment id is "<uid>:<dotted location>". Split at the first colon only;
    // the location itself never contains one.
    const colon = requested.indexOf(":");
    const prefix = colon >= 0 ? requested.substring(0, colon + 1) : "";
    const location = colon >= 0 ? requested.substring(colon + 1) : requested;

    if (location.length === 0) {
        return ids;
    }

    const parts = location.split(".");
    const last = parts[parts.length - 1];

    if (/^\d+$/.test(last)) {
        const head = parts.slice(0, -1);
        for (let n = parseInt(last, 10) - 1; n >= 0; n--) {
            ids.push(prefix + head.concat(String(n)).join("."));
        }
    }

    if (parts.length > 1) {
        ids.push(prefix + parts.slice(0, -1).join("."));
    }

    return ids;
}

/** Remove the notice currently in the page, if any. At most one may exist. */
function remove_existing_notice(): void {
    const existing = document.querySelectorAll("." + NOTICE_CLASS);
    existing.forEach(el => el.remove());
}

/**
 * Build and insert the in-page notice.
 *
 * `used` is the id actually landed on, or null when nothing was found. The
 * fallback form goes as a sibling *before* the resolved paragraph's nearest
 * block-level ancestor — never inside `span.segment`, which in the Columns and
 * Lines layouts is the grid container holding the per-column `colcell` spans,
 * where a block child would add a phantom grid item and break the row's
 * alignment. The give-up form goes at the top of the content instead.
 *
 * The message is real text nodes, not CSS `content:`, so the reader can select
 * it and paste it into a report. The find bar walks it and may splice
 * highlight spans into those text nodes, which is why the dismiss handler is
 * attached to the button and never to a captured text node.
 */
export function show_anchor_notice(requested: string, used: string | null, target: Element | null): HTMLElement {
    remove_existing_notice();

    const notice = document.createElement("div");
    notice.className = NOTICE_CLASS;

    const message = document.createElement("span");
    message.className = "ssp-anchor-notice-message";
    // Full ids ("dn20:4.11"), not the short form printed in the margin: the
    // sentence is meant to be copied into a report, where the sutta must not be
    // left implied.
    message.appendChild(document.createTextNode("Referenced location "));
    const requested_code = document.createElement("code");
    requested_code.appendChild(document.createTextNode(requested));
    message.appendChild(requested_code);
    message.appendChild(document.createTextNode(" not found."));

    if (used !== null) {
        message.appendChild(document.createTextNode(" This location "));
        const used_code = document.createElement("code");
        used_code.appendChild(document.createTextNode(used));
        message.appendChild(used_code);
        message.appendChild(document.createTextNode(" is the closest fallback."));
    }

    notice.appendChild(message);

    const dismiss = document.createElement("button");
    dismiss.className = "ssp-anchor-notice-dismiss";
    dismiss.setAttribute("type", "button");
    dismiss.setAttribute("aria-label", "Dismiss this notice");
    dismiss.appendChild(document.createTextNode("×"));
    dismiss.addEventListener("click", () => notice.remove());
    notice.appendChild(dismiss);

    if (used !== null && target) {
        const block = target.closest("p, li, h1, h2, h3, h4, h5, h6, blockquote") || target;
        const parent = block.parentNode;
        if (parent) {
            parent.insertBefore(notice, block);
            return notice;
        }
    }

    const content = document.getElementById("ssp_content");
    if (content) {
        content.insertBefore(notice, content.firstChild);
    } else if (document.body) {
        document.body.insertBefore(notice, document.body.firstChild);
    }

    return notice;
}

function highlight(el: Element): void {
    document.querySelectorAll("." + HIGHLIGHT_CLASS).forEach(other => {
        other.classList.remove(HIGHLIGHT_CLASS);
    });
    el.classList.add(HIGHLIGHT_CLASS);
    setTimeout(() => el.classList.remove(HIGHLIGHT_CLASS), HIGHLIGHT_MS);
}

/**
 * Scroll to `requested`, or to the nearest preceding sibling the page actually
 * has, and say which happened.
 *
 * Returns "exact", "fallback:<used id>", or "missed". The wrappers log the last
 * two so a support log distinguishes the three outcomes.
 *
 * The walk reads ids from the loaded page rather than from a list computed in
 * Rust: the page is the only authority on which segments the currently
 * displayed text has, which covers the translation-without-that-segment case
 * for free.
 */
export function jump_to_segment(requested: string): string {
    if (!requested || requested.length === 0) {
        return "missed";
    }

    const ids = candidate_ids(requested);
    let found: HTMLElement | null = null;
    let used = "";

    for (const id of ids) {
        // getElementById throughout: a colon is valid in an id but not in a CSS
        // selector fragment, so querySelector would throw on these.
        let el: HTMLElement | null = document.getElementById(id);
        if (!el) {
            // Some pages anchor with `name` rather than `id`. This is the
            // reason the QML wrappers ran a JS pass at all before this module
            // existed; keep it, or those anchors would now count as misses.
            el = document.querySelector('a[name="' + id.replace(/"/g, '\\"') + '"]');
        }
        if (el) {
            found = el;
            used = id;
            break;
        }
    }

    if (!found) {
        // Nothing to scroll to, including a text with no segments at all. Open
        // at the top with the one-sentence notice explaining why.
        show_anchor_notice(requested, null, null);
        window.scrollTo(0, 0);
        return "missed";
    }

    if (used !== requested) {
        // Insert before scrolling, and scroll to the notice rather than to the
        // paragraph: the notice sits above the resolved paragraph, so scrolling
        // to the paragraph itself would leave the explanation off the top edge
        // — unreadable exactly when it is needed. The paragraph lands just
        // below it, still in view.
        const notice = show_anchor_notice(requested, used, found);
        notice.scrollIntoView({ behavior: "auto", block: "start" });
    } else {
        // An exact hit means nothing went wrong; clear a notice left by an
        // earlier miss so it cannot linger on screen.
        remove_existing_notice();
        found.scrollIntoView({ behavior: "auto", block: "start" });
    }

    highlight(found);

    return used === requested ? "exact" : "fallback:" + used;
}

export function init_anchor_jump(): void {
    (window as any).ssp_jump_to_segment = jump_to_segment;
}
