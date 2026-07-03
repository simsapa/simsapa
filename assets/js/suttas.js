async function send_log(msg, log_level) {
    const response = await fetch(`${API_URL}/logger/`, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({
            log_level: log_level,
            msg: msg,
        })
    });

    if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status} ${response.statusText}`);
    }
}

async function log_info(msg) {
    console.log(msg);
    send_log(msg, 'info');
}

async function log_error(msg) {
    console.error(msg);
    send_log(msg, 'error');
}

// Cached text of the last mobile selection before it was auto-cleared by
// the selectionchange timer (see mobile branch below). Used as a fallback
// by HamburgerMenu.getSelectedText() so that menu actions triggered after
// the selection UI has been dismissed still receive the intended text.
// Desktop never writes to this variable.
let last_mobile_selection_text = "";

function selection_text() {
    const selection = document.getSelection();
    let text = "";
    if (selection) {
        text = selection.toString().trim();
    }
    return text;
}

// Clear both selection highlights:
// - the native browser selection (desktop drag, mobile long-press), and
// - the persistent .tapped-word spans we wrap around single-tapped words
//   and finished mobile selections (see highlight_tapped_parts / the mobile
//   selectionchange timer).
//
// Unwrapping by class works regardless of the IS_MOBILE closure scope and
// covers the desktop case (no .tapped-word spans, only the native selection)
// with the same code path. The mobile closure's current_tap_highlight_spans
// array may keep stale references to the removed spans, but that is harmless:
// the next unwrap_tap_highlight() skips any span whose parentNode is null.
function clear_html_selection() {
    // Remove the native browser highlight, if any. On mobile this also
    // fires selectionchange with an empty selection, which cancels any
    // pending 3 s re-highlight timer.
    const selection = document.getSelection ? document.getSelection() : null;
    if (selection) {
        selection.removeAllRanges();
    }

    // Unwrap our own persistent highlight spans back to plain text.
    const spans = document.querySelectorAll('.tapped-word');
    spans.forEach(function (span) {
        const parent = span.parentNode;
        if (!parent) return;
        while (span.firstChild) {
            parent.insertBefore(span.firstChild, span);
        }
        parent.removeChild(span);
    });

    // Drop the cached mobile selection text so subsequent menu actions
    // don't operate on a selection the user just cleared.
    last_mobile_selection_text = "";
}

function lookup_selection() {
    const selected_text = window.getSelection().toString().trim();

    if (!selected_text) {
        console.log('No text selected');
        return;
    }

    fetch(`${API_URL}/lookup_window_query/${encodeURIComponent(selected_text)}`);
}

function summary_selection(explicit_text) {
    let text = "";
    if (typeof explicit_text === "string" && explicit_text.trim() !== "") {
        text = explicit_text.trim();
    } else {
        text = selection_text();
    }
    if (text !== "") {
        fetch(`${API_URL}/summary_query/${WINDOW_ID}/${encodeURIComponent(text)}`);
    }
}

class HamburgerMenu {
    constructor() {
        this.menuButton = document.getElementById('menuButton');
        this.menuDropdown = document.getElementById('menuDropdown');
        this.groupHeaders = document.querySelectorAll('.group-header');
        this.menuItems = document.querySelectorAll('.menu-item');
        this.isOpen = false;

        this.init();
    }

    init() {
        // Menu button click
        this.menuButton.addEventListener('click', () => this.toggleMenu());

        // Group header clicks
        this.groupHeaders.forEach(header => {
            header.addEventListener('click', (e) => this.toggleGroup(e));
            // Prevent text selection clearing
            header.addEventListener('mousedown', (e) => e.preventDefault());
        });

        // Menu item clicks
        this.menuItems.forEach(item => {
            item.addEventListener('click', (e) => this.handleMenuItemClick(e));
            // Prevent text selection clearing
            item.addEventListener('mousedown', (e) => e.preventDefault());
        });

        // Close menu on escape key
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape' && this.isOpen) {
                this.closeMenu();
            }
        });

        // The find bar, the display settings panel and the hamburger menu
        // overlap at the top-right: whichever opens announces itself on
        // 'ssp-panel-open' and the other two close.
        document.addEventListener('ssp-panel-open', (e) => {
            if (e.detail && e.detail.panel !== 'menu' && this.isOpen) {
                this.closeMenu();
            }
        });
    }

    toggleMenu() {
        if (this.isOpen) {
            this.closeMenu();
        } else {
            this.openMenu();
        }
    }

    openMenu() {
        // Close the find bar / display settings panel (see the
        // 'ssp-panel-open' listener in init()).
        document.dispatchEvent(new CustomEvent('ssp-panel-open', { detail: { panel: 'menu' } }));
        this.isOpen = true;
        this.menuButton.classList.add('active');
        this.menuDropdown.classList.add('show');
    }

    closeMenu() {
        this.isOpen = false;
        this.menuButton.classList.remove('active');
        this.menuDropdown.classList.remove('show');
    }

    toggleGroup(e) {
        e.preventDefault();
        e.stopPropagation();

        const header = e.currentTarget;
        const groupName = header.dataset.group;
        const groupItems = document.getElementById(`${groupName}-items`);
        const isExpanded = header.classList.contains('expanded');

        if (isExpanded) {
            // Collapse group
            header.classList.remove('expanded');
            groupItems.classList.remove('show');
        } else {
            // Expand group
            header.classList.add('expanded');
            groupItems.classList.add('show');
        }
    }

    async handleMenuItemClick(e) {
        e.preventDefault();
        e.stopPropagation();

        const action = e.currentTarget.dataset.action;

        // Clearing the selection is a pure webview/DOM operation, so it's
        // handled entirely on the client without a backend round-trip.
        if (action === "clear-selection") {
            clear_html_selection();
            this.closeMenu();
            return;
        }

        let query_text = "";
        if (action === "summarize-sutta") {
            query_text = this.getAllContentText();
        } else {
            query_text = this.getSelectedText();
        }

        try {
            const response = await fetch(`${API_URL}/sutta_menu_action/`, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                },
                body: JSON.stringify({
                    window_id: WINDOW_ID,
                    action: action,
                    text: query_text,
                })
            });

            if (!response.ok) {
                throw new Error(`HTTP error! status: ${response.status} ${response.statusText}`);
            }

            log_info(`Action '${action}' completed`);
        } catch (error) {
            log_error(`Action '${action}' error:` + error);
        }

        this.closeMenu();
    }

    getSelectedText() {
        let text = "";
        if (window.getSelection) {
            text = window.getSelection().toString();
        } else if (document.selection && document.selection.type !== "Control") {
            text = document.selection.createRange().text;
        }
        // On mobile, the selectionchange handler programmatically clears the
        // native selection after 3 s (to dismiss the obscuring system UI)
        // and caches the pre-clear text in `last_mobile_selection_text`.
        // If the live selection is empty, fall back to that cache so the
        // menu's gloss-selection / lookup-selection / copy-selection /
        // translate-selection / analyse-selection actions still receive
        // the text the user had selected.
        if ((!text || text === "") && typeof IS_MOBILE !== "undefined" && IS_MOBILE) {
            text = last_mobile_selection_text || "";
        }
        return text;
    }

    getAllContentText() {
        const content_div = document.getElementById('ssp_content');
        if (!content_div) {
            console.error('Element with id "ssp_content" not found');
            return null;
        }
        // .textContent gets all text content, including text from hidden elements
        // .innerText respects styling and won't include text from hidden elements
        return content_div.innerText || '';
    }
}

function toggle_variant (event) {
    let el = event.target;
    el.parentNode.querySelectorAll(".variant").forEach((i) => {
        i.classList.toggle("hide");
    })
}

function toggle_comment (event) {
    let el = event.target;
    el.parentNode.querySelectorAll(".comment").forEach((i) => {
        i.classList.toggle("hide");
    })
}

class ReadingModeController {
    constructor() {
        this.readingModeButton = document.getElementById('readingModeButton');

        this.init();
    }

    init() {
        if (!this.readingModeButton) {
            return;
        }

        this.readingModeButton.addEventListener('click', () => this.toggle_reading_mode());
    }

    async toggle_reading_mode() {
        // Read current state from the button's class instead of tracking it separately
        const isCurrentlyActive = this.readingModeButton.classList.contains('active');
        const newState = !isCurrentlyActive;

        if (newState) {
            // Activate reading mode
            this.readingModeButton.classList.add('active');

            // Send HTTP request to hide search UI
            try {
                await fetch(`${API_URL}/toggle_reading_mode/${WINDOW_ID}/true`);
            } catch (error) {
                log_error('Failed to toggle reading mode: ' + error);
            }
        } else {
            // Deactivate reading mode
            this.readingModeButton.classList.remove('active');

            // Send HTTP request to show search UI
            try {
                await fetch(`${API_URL}/toggle_reading_mode/${WINDOW_ID}/false`);
            } catch (error) {
                log_error('Failed to toggle reading mode: ' + error);
            }
        }
    }
}

class ChapterNavigationController {
    constructor() {
        this.prevButton = document.getElementById('prevChapterButton');
        this.nextButton = document.getElementById('nextChapterButton');

        this.init();
    }

    init() {
        if (!this.prevButton || !this.nextButton) {
            return;
        }

        // Set initial disabled state based on data attributes
        if (this.prevButton.dataset.isFirst === 'true') {
            this.prevButton.disabled = true;
        }
        if (this.nextButton.dataset.isLast === 'true') {
            this.nextButton.disabled = true;
        }

        // Add click event listeners
        this.prevButton.addEventListener('click', () => this.navigate_prev());
        this.nextButton.addEventListener('click', () => this.navigate_next());
    }

    async navigate_prev() {
        const item_uid = this.prevButton.dataset.spineItemUid;

        // Determine if this is a sutta or book chapter based on which constant is defined
        const is_sutta = typeof SUTTA_UID !== 'undefined';
        const endpoint = is_sutta ? 'prev_sutta' : 'prev_chapter';

        try {
            await fetch(`${API_URL}/${endpoint}/${WINDOW_ID}/${item_uid}`);
        } catch (error) {
            log_error(`Failed to navigate to previous ${is_sutta ? 'sutta' : 'chapter'}: ` + error);
        }
    }

    async navigate_next() {
        const item_uid = this.nextButton.dataset.spineItemUid;

        // Determine if this is a sutta or book chapter based on which constant is defined
        const is_sutta = typeof SUTTA_UID !== 'undefined';
        const endpoint = is_sutta ? 'next_sutta' : 'next_chapter';

        try {
            await fetch(`${API_URL}/${endpoint}/${WINDOW_ID}/${item_uid}`);
        } catch (error) {
            log_error(`Failed to navigate to next ${is_sutta ? 'sutta' : 'chapter'}: ` + error);
        }
    }
}

document.addEventListener("DOMContentLoaded", function(_event) {
    new HamburgerMenu();
    new ReadingModeController();
    new ChapterNavigationController();
    if (IS_MOBILE) {
        // On mobile in a WebView there is no dblclick event. Use a single tap:
        // resolve the caret position under the tap, extract the word at that
        // offset, and pass it directly to summary_selection().
        document.body.classList.add('mobile');

        function caret_node_offset_from_point(x, y) {
            let node = null;
            let offset = 0;
            if (typeof document.caretPositionFromPoint === "function") {
                const pos = document.caretPositionFromPoint(x, y);
                if (!pos) return null;
                node = pos.offsetNode;
                offset = pos.offset;

            // caretRangeFromPoint is deprecated but is the only caret-from-point
            // API on older WebKit/Chromium WebViews that lack the standard
            // caretPositionFromPoint, so we keep it as a fallback for tap-to-select.
            } else if (typeof document.caretRangeFromPoint === "function") {
                const range = document.caretRangeFromPoint(x, y);
                if (!range) return null;
                node = range.startContainer;
                offset = range.startOffset;
            } else {
                return null;
            }
            if (!node || node.nodeType !== Node.TEXT_NODE) return null;
            return { node: node, offset: offset };
        }

        function word_at_offset(text_node, offset) {
            const value = text_node.nodeValue || "";
            const re = /[\p{L}\p{M}'’]+/gu;
            let match;
            while ((match = re.exec(value)) !== null) {
                const start = match.index;
                const end = start + match[0].length;
                if (offset >= start && offset <= end) {
                    return { word: match[0], start: start, end: end };
                }
            }
            return { word: "", start: -1, end: -1 };
        }

        // Spans belonging to the currently visible highlight, if any.
        // An array because a multi-element selection is rendered as one
        // span per intersecting text node. The highlight is persistent:
        // it stays on the word/selection the user last acted on (acting
        // as a reading anchor) until the next single-tap or selection
        // replaces it, rather than fading after a fixed timeout.
        let current_tap_highlight_spans = [];

        function unwrap_tap_highlight() {
            const spans = current_tap_highlight_spans;
            current_tap_highlight_spans = [];
            for (let i = 0; i < spans.length; i++) {
                const span = spans[i];
                const parent = span.parentNode;
                if (!parent) continue;
                while (span.firstChild) {
                    parent.insertBefore(span.firstChild, span);
                }
                parent.removeChild(span);
                // NOTE: we intentionally do NOT call `parent.normalize()`
                // here. The find-bar (src-ts/find.ts) uses
                // `dom-find-and-replace`, which keeps a recover closure
                // referencing the text nodes it wrapped. Merging adjacent
                // text nodes inside #ssp_content could invalidate those
                // cached references. Leaving adjacent text nodes unmerged
                // is harmless — the DOM renders identically — and at
                // worst a subsequent tap lands in a shorter text node,
                // which word_at_offset handles correctly.
            }
        }

        // Collect the text-node-level slices covered by a Range.
        //
        // A Range may span multiple elements (e.g. a long-press + drag
        // selection that crosses <span> boundaries within a paragraph,
        // or even across paragraphs). surroundContents() only works on
        // ranges within a single text node, so we walk all intersecting
        // text nodes and produce one {text_node, start, end} slice per
        // node — the start/end of each slice is clamped by the range's
        // endpoints. Each slice is later wrapped in its own .tapped-word
        // span. Visually the result reads as a single highlighted run,
        // since the intervening element tags render transparently.
        function collect_range_text_parts(range) {
            const parts = [];
            if (!range || range.collapsed) return parts;

            const root = range.commonAncestorContainer;

            // Single-text-node range: the walker below would yield
            // nothing (text nodes have no descendants), so handle
            // directly.
            if (root.nodeType === Node.TEXT_NODE) {
                if (range.startOffset < range.endOffset) {
                    parts.push({
                        text_node: root,
                        start: range.startOffset,
                        end: range.endOffset,
                    });
                }
                return parts;
            }

            const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, null);
            let node;
            while ((node = walker.nextNode()) !== null) {
                // Determine whether this text node intersects the range.
                // We compare the range boundaries against a node-spanning
                // range. If either the range ends before the node begins
                // or starts after the node ends, there's no intersection.
                const node_range = document.createRange();
                node_range.selectNodeContents(node);
                const ends_before = range.compareBoundaryPoints(Range.END_TO_START, node_range) >= 0;
                const starts_after = range.compareBoundaryPoints(Range.START_TO_END, node_range) <= 0;
                if (ends_before || starts_after) continue;

                const value = node.nodeValue || "";
                let start = 0;
                let end = value.length;
                if (node === range.startContainer) start = range.startOffset;
                if (node === range.endContainer) end = range.endOffset;
                if (start >= end) continue;
                // Skip whitespace-only slices. A drag that crosses block
                // boundaries traverses the text nodes the browser inserts
                // between <p> etc.; wrapping those in a background-coloured
                // span would render as a thin yellow gap between paragraphs.
                // The lookup itself is unaffected — those slices were
                // already stripped by .trim() when we built the query.
                if (value.slice(start, end).trim() === "") continue;
                parts.push({ text_node: node, start: start, end: end });
            }
            return parts;
        }

        function highlight_tapped_parts(parts) {
            // Replace any previous highlight before painting the new one
            // so at most one highlight set exists in the DOM and we don't
            // nest .tapped-word spans.
            unwrap_tap_highlight();
            const created = [];
            for (let i = 0; i < parts.length; i++) {
                const p = parts[i];
                try {
                    // surroundContents only affects the single text node
                    // it wraps; earlier iterations don't invalidate the
                    // text_node references held by later parts (those
                    // point at different nodes).
                    const range = document.createRange();
                    range.setStart(p.text_node, p.start);
                    range.setEnd(p.text_node, p.end);
                    const span = document.createElement('span');
                    span.className = 'tapped-word';
                    range.surroundContents(span);
                    created.push(span);
                } catch (e) {
                    // One bad slice shouldn't drop the whole highlight.
                    log_error("highlight_tapped_parts segment: " + e);
                }
            }
            current_tap_highlight_spans = created;
        }

        function highlight_tapped_word(text_node, start, end) {
            highlight_tapped_parts([{ text_node: text_node, start: start, end: end }]);
        }

        // Timestamp of the last accepted tap. The highlight is persistent
        // (replaced on next tap, not auto-removed), but we still debounce
        // very rapid taps to avoid two click events from the same user
        // gesture both being treated as lookups.
        let last_tap_ms = 0;

        document.addEventListener("click", function (event) {
            try {
                // Synthetic clicks (keyboard activation, accessibility
                // tooling) arrive with `event.detail === 0` and
                // clientX/Y === 0. Without this guard they would resolve
                // to whatever text is at the viewport's top-left and
                // trigger a spurious lookup. Real pointer taps always
                // have `detail >= 1`.
                if (event.detail === 0) return;

                if (event.target.closest('#findContainer, a, button, #menuButton, .menu-dropdown, .variant-wrap, [contenteditable], input, textarea')) {
                    return;
                }
                // If the user has an active selection (e.g. from long-press),
                // leave it alone so menu actions can read it and so the
                // selectionchange handler below drives the lookup instead.
                const sel = document.getSelection();
                if (sel && sel.toString().trim() !== "") {
                    return;
                }

                // Debounce: ignore taps that arrive within 250 ms of the
                // previous accepted tap, to absorb stray duplicate click
                // events from the same user gesture.
                const now = (typeof performance !== "undefined" && performance.now) ? performance.now() : Date.now();
                if (now - last_tap_ms < 250) return;

                const hit = caret_node_offset_from_point(event.clientX, event.clientY);
                if (!hit) return;
                const found = word_at_offset(hit.node, hit.offset);
                const word = (found.word || "").trim();
                if (word === "") return;

                last_tap_ms = now;
                // The tapped word becomes the menu's fallback selection
                // text too, so Lookup/Gloss/Copy/... selection actions
                // triggered right after a tap operate on that word.
                last_mobile_selection_text = word;
                highlight_tapped_word(hit.node, found.start, found.end);
                summary_selection(word);
            } catch (e) {
                log_error("mobile tap-to-lookup: " + e);
            }
        });

        // Selection-driven lookup on mobile.
        //
        // Split of responsibilities:
        // - The click handler above covers the common "tap a word" case. Its
        //   word regex does not include `-`, so hyphenated Pāli compounds
        //   (e.g. "anatta-lakkhaṇa") cannot be looked up by tapping alone.
        // - This selectionchange handler covers long-press to select, and
        //   drag-to-extend the selection — the lookup query follows the
        //   selection live, same as the desktop flow. This is the only way
        //   to look up hyphenated compounds or multi-word phrases on mobile.
        //
        // We intentionally do NOT reintroduce the previous_selection_text /
        // word_summary_was_closed boundary-drag workaround from before the
        // single-tap PRD. With single-tap covering the zero-selection case,
        // the simple "selection non-empty → lookup" rule is enough: dragging
        // the selection handles fires selectionchange repeatedly with the
        // updated text, which is exactly what we want.
        //
        // System-UI dismissal: once the user has finished adjusting their
        // selection, the WebView's selection handles and ActionMode / callout
        // toolbar obscure the app UI (especially WordSummary). We cannot
        // hide that UI via CSS while a live selection exists. Instead, we
        // start a 3-second debounced timer on each selectionchange: when
        // the user stops modifying the selection, the timer fires, captures
        // the selected text into `last_mobile_selection_text` (so menu
        // actions can still read it), clears the native selection
        // (dismissing the system UI), and replaces the visual feedback with
        // our own persistent .tapped-word highlight over the same range.
        let selection_clear_timer = null;

        // Throttle timestamp for the live lookup. selectionchange fires
        // tens of times per second while the user drags selection handles;
        // without a throttle each fire would produce a separate
        // /summary_query fetch. 30 ms caps the fetch cadence at ~33/s
        // while still feeling instantaneous to the user (≤1 frame at
        // 30 fps) so WordSummary tracks the selection during the drag.
        let last_live_lookup_ms = 0;

        document.addEventListener("selectionchange", function (_event) {
            // Defensive try/catch. compareBoundaryPoints / Range APIs can
            // throw on disconnected or mid-mutation ranges — e.g. when
            // #ssp_content is replaced during sutta navigation while a
            // selection existed. Without this, a throw would leave the
            // handler broken for subsequent events until the page is
            // reloaded.
            try {
                const selection = document.getSelection();
                if (!selection || selection.rangeCount === 0) {
                    // Selection gone — either the user tapped away or our
                    // 3 s timer called removeAllRanges(). Drop the pending
                    // timer so we don't act on stale state.
                    if (selection_clear_timer !== null) {
                        clearTimeout(selection_clear_timer);
                        selection_clear_timer = null;
                    }
                    return;
                }

                const range = selection.getRangeAt(0);
                const container = range.commonAncestorContainer;
                const element = container.nodeType === Node.TEXT_NODE ? container.parentElement : container;
                // Skip selections inside the find bar or form inputs.
                if (element && element.closest('#findContainer, [contenteditable], input, textarea')) {
                    return;
                }

                const text = selection.toString().trim();
                if (text === "") {
                    if (selection_clear_timer !== null) {
                        clearTimeout(selection_clear_timer);
                        selection_clear_timer = null;
                    }
                    return;
                }

                // Cache the selected text so HamburgerMenu.getSelectedText()
                // can retrieve it after we clear the live selection. This
                // must update on every event — it's O(1) and the menu may
                // be opened at any moment.
                last_mobile_selection_text = text;

                // Throttled live lookup. `summary_selection()` reads the
                // current selection via selection_text() and fetches
                // /summary_query — we skip most fires during a rapid drag.
                const now = (typeof performance !== "undefined" && performance.now) ? performance.now() : Date.now();
                if (now - last_live_lookup_ms >= 30) {
                    last_live_lookup_ms = now;
                    summary_selection();
                }

                // (Re)start the 3 s debounce timer. Every new
                // selectionchange pushes the deadline out, so the timer
                // only fires once the user has stopped adjusting the
                // selection.
                //
                // Perf note: we intentionally do NOT call
                // collect_range_text_parts(range) here. That function
                // walks every text node under commonAncestorContainer —
                // on a long sutta with multi-paragraph drags that's
                // thousands of nodes, and per-event it dominates the
                // cost of a drag. Instead we defer the walk to the timer
                // callback below, which re-reads the live selection once
                // at fire time. The highlight paint is also deferred into
                // that single batch, so multi-paragraph selections produce
                // one reflow instead of per-event churn.
                if (selection_clear_timer !== null) {
                    clearTimeout(selection_clear_timer);
                }
                selection_clear_timer = setTimeout(function () {
                    selection_clear_timer = null;
                    try {
                        // Re-read the current selection at fire time.
                        // If the user dismissed it (via back button, tap
                        // outside, etc.), there's nothing to do.
                        const live = document.getSelection();
                        let parts = [];
                        if (live && live.rangeCount > 0) {
                            const live_range = live.getRangeAt(0);
                            if (!live_range.collapsed) {
                                parts = collect_range_text_parts(live_range);
                            }
                        }

                        // Drop parts whose text nodes are no longer in
                        // the document. This happens when the user
                        // navigates to a different sutta within the 3 s
                        // window — #ssp_content is replaced and the
                        // captured text nodes are detached. Calling
                        // surroundContents on detached nodes is a silent
                        // no-op (Range APIs don't throw), but skipping
                        // them avoids wasted work and keeps the intent
                        // of the code obvious to future readers.
                        parts = parts.filter(function (p) {
                            return p.text_node && p.text_node.isConnected;
                        });

                        if (live) live.removeAllRanges();

                        // Replace the dismissed native selection with our
                        // own persistent highlight so the user can still
                        // see what was looked up. highlight_tapped_parts
                        // unwraps any previous .tapped-word spans first,
                        // so at most one highlight set exists in the DOM
                        // at a time.
                        if (parts.length > 0) {
                            highlight_tapped_parts(parts);
                        }
                    } catch (e) {
                        log_error("mobile selection auto-clear: " + e);
                    }
                }, 3000);
            } catch (e) {
                log_error("mobile selectionchange: " + e);
            }
        });

    } else {
        // On desktop, double click works to select a word and trigger a lookup.
        // Double click always triggers a lookup.
        document.addEventListener("dblclick", function (event) {
            // Skip if double click is within find bar
            if (event.target.closest('#findContainer')) {
                return;
            }
            summary_selection();
        });
    }

    ssp_rebind_content_handlers();
});

// Re-binds the per-node handlers inside #ssp_content. Called at
// DOMContentLoaded, and again by the webpack bundle's
// reinit_sutta_content() (src-ts/content_reload.ts) after a content-block
// swap replaces those nodes. The document-level delegated handlers above
// (click / selectionchange / dblclick) survive swaps and must not be
// re-registered here.
function ssp_rebind_content_handlers() {
    document.querySelectorAll(".variant-wrap .mark").forEach((i) => {
        i.addEventListener("click", toggle_variant);
    });
    document.querySelectorAll(".comment-wrap .mark").forEach((i) => {
        i.addEventListener("click", toggle_comment);
    });
}
window.ssp_rebind_content_handlers = ssp_rebind_content_handlers;
