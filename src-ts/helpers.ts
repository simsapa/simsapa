// Regex pattern for matching sutta references in text
// Ported from backend/src/helpers.rs:25-27
const RE_ALL_BOOK_SUTTA_REF = /\b(DN|MN|SN|AN|Pv|Vv|Vism|iti|kp|khp|snp|th|thag|thig|ud|uda|dhp)[ .]*(\d[\d.:]*)\b/i;

// Import the confirmation modal function
import { show_external_link_confirmation } from "./confirm_modal";
import { show_footnote, footnote_modal } from "./footnote_modal";
import { invalid_link_modal } from "./invalid_link_modal";

/**
 * Send log message to backend logger
 */
async function send_log(msg: string, log_level: string): Promise<void> {
    const API_URL = (globalThis as any).API_URL || 'http://localhost:4848';
    const response = await fetch(`${API_URL}/logger`, {
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

async function log_info(msg: string): Promise<void> {
    console.log(msg);
    send_log(msg, 'info');
}

async function log_error(msg: string): Promise<void> {
    console.error(msg);
    send_log(msg, 'error');
}

/**
 * Opens an external URL using the backend API
 * This uses Qt's QDesktopServices to open the URL in the system browser
 */
async function open_external_url(url: string): Promise<void> {
    const API_URL = (globalThis as any).API_URL || 'http://localhost:4848';
    try {
        const response = await fetch(`${API_URL}/open_external_url`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({ url: url })
        });

        if (!response.ok) {
            log_error(`Failed to open external URL: ${response.status}`);
        }
    } catch (error) {
        const errorMsg = error instanceof Error ? error.message : String(error);
        log_error(`Error opening external URL: ${errorMsg}`);
    }
}

function show_transient_message(text: string, msg_div_id: string): void {
    const div = document.createElement('div');
    div.className = 'message';

    const content = document.createElement('div');
    content.className = 'msg-content';
    content.textContent = text;
    div.appendChild(content);

    let el = document.getElementById(msg_div_id);
    if (!el) {
        console.error("Cannot find: transient-messages");
        return;
    }

    el.appendChild(div);

    div.style.transition = 'opacity 1.5s ease-in-out';
    div.style.opacity = '1';

    // After 3 seconds, start fading out
    setTimeout(() => {
        div.style.opacity = '0';
    }, 1000);

    // After the transition ends, remove the div from the DOM
    div.addEventListener('transitionend', () => {
        div.remove();
    });
}

/**
 * Extract sutta UID and optional anchor from an anchor element
 * Returns object with uid and anchor, or null if not found
 * Format: { uid: string, anchor?: string }
 */
function extract_sutta_uid_from_link(anchor: HTMLAnchorElement): { uid: string, anchor?: string } | null {
    const href = anchor.getAttribute('href') || '';
    const text = anchor.textContent || '';

    // Helper to extract anchor from URL
    const extract_anchor = (url: string): string | undefined => {
        const hash_index = url.indexOf('#');
        if (hash_index !== -1) {
            const anchor_part = url.substring(hash_index + 1);
            return anchor_part || undefined;
        }
        return undefined;
    };

    // Helper to remove anchor from URL
    const remove_anchor = (url: string): string => {
        const hash_index = url.indexOf('#');
        return hash_index !== -1 ? url.substring(0, hash_index) : url;
    };

    // Priority 1: ssp:// protocol
    // Format: ssp://suttas/{uid} where uid can be sn47.8/en/thanissaro or sn47.8/en/thanissaro#anchor
    if (href.startsWith('ssp://')) {
        const match = href.match(/^ssp:\/\/suttas\/(.+)$/);
        if (match) {
            const full_path = match[1];
            const anchor_id = extract_anchor(full_path);
            const uid = remove_anchor(full_path);
            return { uid, anchor: anchor_id };
        }
    }

    // Priority 2: Suttacentral URL
    // Format: https://suttacentral.net/sn56.11/en/bodhi or https://suttacentral.net/mn12/en/sujato#37.5
    if (href.includes('suttacentral.net')) {
        const match = href.match(/suttacentral\.net\/(.+)$/);
        if (match) {
            const full_path = match[1];
            const anchor_id = extract_anchor(full_path);
            const uid = remove_anchor(full_path);
            return { uid, anchor: anchor_id };
        }
    }

    // Priority 3: thebuddhaswords.net URL
    // Format 1: https://thebuddhaswords.net/suttas/an4.41.html
    // Format 2: https://thebuddhaswords.net/dn/dn11.html (dictionary links)
    // Format 3: https://thebuddhaswords.net/tha/tha3.html with text TH179 (verse-based)
    if (href.includes('thebuddhaswords.net')) {
        // Try format 1: /suttas/...
        let match = href.match(/\/suttas\/([^.#]+)\.html/);
        if (match) {
            const uid = `${match[1]}/pli/ms`;
            const anchor_id = extract_anchor(href);
            return { uid, anchor: anchor_id };
        }

        // Try format 2: /collection/code.html (e.g., /dn/dn11.html, /sn/sn35.93.html)
        match = href.match(/\/([a-z]+)\/([a-z0-9.]+)\.html/);
        if (match) {
            const collection = match[1];
            const code = match[2];
            const anchor_id = extract_anchor(href);

            // Handle verse-based texts (tha, thi, it) by checking link text
            if (collection === 'tha' || collection === 'thi' || collection === 'it') {
                // Try to extract verse number from link text (e.g., "TH179", "THI71", "ITI16")
                const verse_match = text.match(/^(TH|THI|ITI)(\d+)$/);
                if (verse_match) {
                    const book = verse_match[1].toLowerCase();
                    const verse_num = verse_match[2];

                    // For verse-based texts, construct UID directly
                    // The Rust backend helpers will handle the conversion
                    // For now, we'll use a simple format that the backend can process
                    if (book === 'th') {
                        // TH179 → thag verse 179 (backend will convert to proper UID)
                        const uid = `thag${verse_num}/pli/ms`;
                        return { uid, anchor: anchor_id };
                    } else if (book === 'thi') {
                        // THI71 → thig verse 71
                        const uid = `thig${verse_num}/pli/ms`;
                        return { uid, anchor: anchor_id };
                    } else if (book === 'iti') {
                        // ITI16 → iti16
                        const uid = `iti${verse_num}/pli/ms`;
                        return { uid, anchor: anchor_id };
                    }
                }
            }

            // For standard suttas, use the code from the filename
            const uid = `${code}/pli/ms`;
            return { uid, anchor: anchor_id };
        }
    }

    // Priority 4: Text-based reference (e.g., "SN 56.11" or "MN 10")
    // Convert colons to dots before matching, 'AN 5:114' to 'AN 5.114'
    const normalized_text = text.replace(/:/g, '.');
    const match = normalized_text.match(RE_ALL_BOOK_SUTTA_REF);
    if (match) {
        const book = match[1].toLowerCase();
        const number = match[2];
        // Construct UID with /pli/ms fallback
        const uid = `${book}${number}/pli/ms`;
        const anchor_id = extract_anchor(href);
        return { uid, anchor: anchor_id };
    }

    return null;
}

/**
 * Opens a sutta by UID through the backend API
 * Makes GET request to /open_sutta_window/{uid}
 */
async function open_sutta_by_uid(uid: string, original_url?: string): Promise<void> {
    // API_URL is defined as a global const in page.html template
    const API_URL = (globalThis as any).API_URL || 'http://localhost:4848';

    try {
        // Don't encode slashes - Rocket's <uid..> path parameter expects them as-is
        const url = `${API_URL}/open_sutta_window/${uid}`;
        const response = await fetch(url);

        if (response.status === 404) {
            // Sutta not found - show error dialog with option to open external link
            log_error(`Sutta not found: ${uid}`);
            let message = `Sutta not found in database: ${uid}`;
            if (original_url) {
                message += `\n\nOriginal URL: ${original_url}\n\nWould you like to open this link in your web browser?`;

                // Close footnote modal if it's open before showing external link confirmation
                if (footnote_modal.is_visible()) {
                    footnote_modal.close();
                }

                const confirmed = await show_external_link_confirmation(original_url);
                if (confirmed) {
                    window.open(original_url, '_blank');
                }
            } else {
                alert(message);
            }
        } else if (!response.ok) {
            log_error(`Failed to open sutta ${uid}: ${response.status}`);
        } else {
            log_info(`Successfully opened sutta ${uid}`);
        }
    } catch (error) {
        const errorMsg = error instanceof Error ? error.message : String(error);
        log_error(`Error opening sutta ${uid}: ${errorMsg}`);
    }
}

/**
 * Opens a sutta by UID in a new tab in the current window
 * Makes GET request to /open_sutta_tab/{window_id}/{uid}?anchor={anchor}
 */
async function open_sutta_in_tab(uid: string, original_url?: string, anchor?: string): Promise<void> {
    // API_URL and WINDOW_ID are defined as global consts in page.html template
    // Try multiple ways to access these globals
    const win = window as any;

    const API_URL = win.API_URL || (globalThis as any).API_URL || 'http://localhost:4848';
    const WINDOW_ID = win.WINDOW_ID || (globalThis as any).WINDOW_ID;

    // If WINDOW_ID is not defined, fall back to opening in a new window
    if (!WINDOW_ID || WINDOW_ID === '') {
        await log_error(`WINDOW_ID not defined, falling back to open_sutta_by_uid for uid='${uid}'`);
        await open_sutta_by_uid(uid, original_url);
        return;
    }

    await log_info(`open_sutta_in_tab: WINDOW_ID='${WINDOW_ID}', uid='${uid}'${anchor ? `, anchor='${anchor}'` : ''}`);

    try {
        // Don't encode slashes - Rocket's <uid..> path parameter expects them as-is
        let url = `${API_URL}/open_sutta_tab/${WINDOW_ID}/${uid}`;

        // Add anchor as query parameter if present
        if (anchor) {
            url += `?anchor=${encodeURIComponent(anchor)}`;
        }

        const response = await fetch(url);

        if (response.status === 404) {
            // Sutta not found - show error dialog with option to open external link
            log_error(`Sutta not found: ${uid}`);
            let message = `Sutta not found in database: ${uid}`;
            if (original_url) {
                message += `\n\nOriginal URL: ${original_url}\n\nWould you like to open this link in your web browser?`;

                // Close footnote modal if it's open before showing external link confirmation
                if (footnote_modal.is_visible()) {
                    footnote_modal.close();
                }

                const confirmed = await show_external_link_confirmation(original_url);
                if (confirmed) {
                    await open_external_url(original_url);
                }
            } else {
                alert(message);
            }
        } else if (!response.ok) {
            log_error(`Failed to open sutta ${uid}: ${response.status}`);
        }
    } catch (error) {
        const errorMsg = error instanceof Error ? error.message : String(error);
        log_error(`Error opening sutta ${uid}: ${errorMsg}`);
    }
}

/**
 * Opens a book page in a new tab in the current window
 * Makes GET request to /open_book_page_tab/{window_id} with the book page URL
 */
async function open_book_page_in_tab(book_page_url: string): Promise<void> {
    // API_URL and WINDOW_ID are defined as global consts in page.html template
    const win = window as any;
    const API_URL = win.API_URL || (globalThis as any).API_URL || 'http://localhost:4848';
    const WINDOW_ID = win.WINDOW_ID || (globalThis as any).WINDOW_ID;

    // If WINDOW_ID is not defined, just navigate to the page normally
    if (!WINDOW_ID || WINDOW_ID === '') {
        await log_info(`WINDOW_ID not defined, navigating to book page: ${book_page_url}`);
        window.location.href = book_page_url;
        return;
    }

    await log_info(`open_book_page_in_tab: WINDOW_ID='${WINDOW_ID}', url='${book_page_url}'`);

    try {
        // Send the book page URL to the backend to open in a new tab
        const url = `${API_URL}/open_book_page_tab/${WINDOW_ID}`;
        const response = await fetch(url, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({ book_page_url: book_page_url })
        });

        if (!response.ok) {
            await log_error(`Failed to open book page ${book_page_url}: ${response.status}`);
        }
    } catch (error) {
        const errorMsg = error instanceof Error ? error.message : String(error);
        await log_error(`Error opening book page ${book_page_url}: ${errorMsg}`);
    }
}

/**
 * Triggers a DPPN-only Fulltext Match query in the dictionary tab without
 * touching the user's current search input / mode / filter UI state.
 * Backed by POST /dppn_lookup.
 */
async function run_dppn_lookup(query: string): Promise<void> {
    const win = window as any;
    const API_URL = win.API_URL || (globalThis as any).API_URL || 'http://localhost:4848';
    const WINDOW_ID = win.WINDOW_ID || (globalThis as any).WINDOW_ID || '';

    try {
        const response = await fetch(`${API_URL}/dppn_lookup`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({ window_id: WINDOW_ID, query: query })
        });

        if (!response.ok) {
            await log_error(`Failed DPPN lookup for '${query}': ${response.status}`);
        }
    } catch (error) {
        const errorMsg = error instanceof Error ? error.message : String(error);
        await log_error(`Error during DPPN lookup for '${query}': ${errorMsg}`);
    }
}

/**
 * Triggers a Combined dictionary lookup (DPD lookup + word deconstructor) in
 * the dictionary tab for the given word. Used by the DPD EPD word-list links
 * (ssp://word_lookup/...). Backed by POST /word_lookup.
 */
async function run_word_lookup(word: string): Promise<void> {
    const win = window as any;
    const API_URL = win.API_URL || (globalThis as any).API_URL || 'http://localhost:4848';
    const WINDOW_ID = win.WINDOW_ID || (globalThis as any).WINDOW_ID || '';

    try {
        const response = await fetch(`${API_URL}/word_lookup`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({ window_id: WINDOW_ID, query: word })
        });

        if (!response.ok) {
            await log_error(`Failed word lookup for '${word}': ${response.status}`);
        }
    } catch (error) {
        const errorMsg = error instanceof Error ? error.message : String(error);
        await log_error(`Error during word lookup for '${word}': ${errorMsg}`);
    }
}

/**
 * Shows a confirmation dialog for external links
 * Returns a Promise that resolves to true if user confirms, false otherwise
 * NOTE: This function is now imported from confirm_modal.ts
 * Import statement at top of file: import { show_external_link_confirmation } from "./confirm_modal";
 */

/**
 * Handles link clicks and classifies them into:
 * - Anchor links (same page) - default behavior
 * - Sutta links - call API to open sutta in tab
 * - Book page links (to different resource) - open in new tab
 * - External links - show confirmation before opening
 */
async function handle_link_click(event: MouseEvent): Promise<void> {
    const target = event.target as HTMLElement;

    // Find the anchor element (might be a child element that was clicked)
    let anchor: HTMLAnchorElement | null = null;
    if (target.tagName === 'A') {
        anchor = target as HTMLAnchorElement;
    } else {
        anchor = target.closest('a');
    }

    if (!anchor) {
        return;
    }

    const href = anchor.getAttribute('href') || '';

    // Case 1: Footnote links - show in modal instead of jumping to footnote
    if (href.startsWith('#')) {
        // Try to handle as a footnote
        if (show_footnote(anchor)) {
            event.preventDefault();
            return;
        }
        // If not a footnote, allow default anchor link behavior
        return;
    }

    // Case 2a: DPPN cross-reference lookup link
    // Format: ssp://dppn_lookup/<percent-encoded-query>
    if (href.startsWith('ssp://dppn_lookup/')) {
        event.preventDefault();
        const encoded = href.substring('ssp://dppn_lookup/'.length);
        let query: string;
        try {
            query = decodeURIComponent(encoded);
        } catch (_) {
            query = encoded;
        }
        await run_dppn_lookup(query);
        return;
    }

    // Case 2b: EPD word-list lookup link (DPD English->Pāḷi word items)
    // Format: ssp://word_lookup/<percent-encoded-word>
    // Triggers a Combined dictionary lookup for the word.
    if (href.startsWith('ssp://word_lookup/')) {
        event.preventDefault();
        const encoded = href.substring('ssp://word_lookup/'.length);
        let word: string;
        try {
            word = decodeURIComponent(encoded);
        } catch (_) {
            word = encoded;
        }
        await run_word_lookup(word);
        return;
    }

    // Case 2: Try to extract sutta UID and anchor
    const sutta_result = extract_sutta_uid_from_link(anchor);
    if (sutta_result) {
        event.preventDefault();
        // Pass the original href so we can offer to open it if sutta not found
        // Open in tab instead of new window, including anchor if present
        await open_sutta_in_tab(sutta_result.uid, href, sutta_result.anchor);
        return;
    }

    // Case 3: Book page links to different resources
    // Format: /book_pages/<book_uid>/<resource_path>
    if (href.startsWith('/book_pages/')) {
        const current_url = window.location.pathname;
        // Extract the path without the fragment
        const href_without_fragment = href.split('#')[0];
        const current_without_fragment = current_url.split('#')[0];

        // If it's a link to a different resource (not just an anchor on same page)
        if (href_without_fragment !== current_without_fragment && href_without_fragment !== '') {
            event.preventDefault();
            await open_book_page_in_tab(href);
            return;
        }
        // Otherwise, it's either an anchor on the same page or the same page, allow default behavior
        return;
    }

    // Case 4: External links - show confirmation
    if (href.startsWith('http://') || href.startsWith('https://')) {
        event.preventDefault();

        // Close footnote modal if it's open before showing external link confirmation
        if (footnote_modal.is_visible()) {
            footnote_modal.close();
        }

        const confirmed = await show_external_link_confirmation(href);
        if (confirmed) {
            await open_external_url(href);
        }
        return;
    }

    // Case 5: Invalid localhost links - show warning modal
    if (href.startsWith('/')) {
        event.preventDefault();

        // Close footnote modal if it's open before showing invalid link modal
        if (footnote_modal.is_visible()) {
            footnote_modal.close();
        }

        invalid_link_modal.show(href);
        return;
    }
}

export {
    show_transient_message,
    extract_sutta_uid_from_link,
    open_sutta_by_uid,
    open_sutta_in_tab,
    open_book_page_in_tab,
    open_external_url,
    run_dppn_lookup,
    run_word_lookup,
    handle_link_click,
    log_info,
    log_error,
    invalid_link_modal,
}
