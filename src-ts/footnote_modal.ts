/**
 * Footnote modal for displaying footnote definitions in a pop-up
 * Shows footnote content without jumping to the footnote location
 */

import { attach_link_handlers_to_element } from "./simsapa";

class FootnoteModal {
    private overlay: HTMLElement | null = null;
    private title_element: HTMLElement | null = null;
    private body_element: HTMLElement | null = null;
    private close_button: HTMLElement | null = null;
    private initialized: boolean = false;
    private handlers_bound: boolean = false;

    constructor() {
        // Don't initialize in constructor - wait for DOM to be ready
    }

    private init(): void {
        // Get current DOM elements
        const overlay = document.getElementById('footnoteModalOverlay');
        const title_element = document.getElementById('footnoteModalTitle');
        const body_element = document.getElementById('footnoteModalBody');
        const close_button = document.getElementById('footnoteModalClose');

        if (!overlay || !title_element || !body_element || !close_button) {
            console.error('[FootnoteModal] Required elements not found in DOM');
            this.initialized = false;
            return;
        }

        // Check if we need to reinitialize (DOM elements changed)
        const needs_reinit = (
            this.overlay !== overlay ||
            this.close_button !== close_button
        );

        if (needs_reinit || !this.initialized) {
            // Update references
            this.overlay = overlay;
            this.title_element = title_element;
            this.body_element = body_element;
            this.close_button = close_button;

            // Bind event handlers
            this.close_button.addEventListener('click', () => this.close());
            this.overlay.addEventListener('click', (e) => this.handle_overlay_click(e));

            // Handle escape key - only bind this once globally
            if (!this.handlers_bound) {
                document.addEventListener('keydown', (e) => this.handle_keydown(e));
                this.handlers_bound = true;
            }

            this.initialized = true;
        }
    }

    /**
     * Show the footnote modal with footnote number and content
     */
    public show(footnote_number: string, footnote_content: string): void {
        // Always try to initialize (will skip if already initialized with valid DOM)
        this.init();

        if (!this.overlay || !this.title_element || !this.body_element) {
            console.error('[FootnoteModal] Modal not initialized');
            return;
        }

        // Set the title with footnote number
        this.title_element.textContent = `Note ${footnote_number}`;

        // Set the body content (as HTML to preserve formatting)
        this.body_element.innerHTML = footnote_content;

        // Attach link handlers to links within the footnote content
        // This ensures sutta links and external links work properly from within the modal
        attach_link_handlers_to_element(this.body_element);

        // Show the modal
        this.overlay.classList.add('show');
    }

    private handle_overlay_click(e: MouseEvent): void {
        // Close modal if clicking directly on overlay (not on modal content)
        if (e.target === this.overlay) {
            this.close();
        }
    }

    private handle_keydown(e: KeyboardEvent): void {
        // Close modal on Escape key
        if (e.key === 'Escape' && this.overlay?.classList.contains('show')) {
            this.close();
        }
    }

    /**
     * Close the footnote modal
     * Public so it can be called from other modules (e.g., before showing external link modal)
     */
    public close(): void {
        if (!this.overlay) {
            return;
        }

        this.overlay.classList.remove('show');
    }

    /**
     * Check if the modal is currently visible
     */
    public is_visible(): boolean {
        return this.overlay?.classList.contains('show') || false;
    }
}

// Export singleton instance
export const footnote_modal = new FootnoteModal();

/** The footnote reference links of every markup source on a page. */
export const FOOTNOTE_LINK_SELECTOR = '.footnote-reference a[href^="#"], .fn a[href^="#"]';

/**
 * The number of a footnote reference: its `data-n` attribute (set by
 * `normalise_footnote_markers()`), else its text with brackets removed
 * ("9", "[2]").
 */
export function extract_footnote_number(anchor: HTMLAnchorElement): string {
    const n = anchor.getAttribute('data-n');
    if (n !== null && n !== '') {
        return n;
    }
    return (anchor.textContent || '').replace(/[\[\]]/g, '').trim();
}

/**
 * Moves each footnote reference's number from its text into `data-n`, and
 * empties the text; CSS draws it (`::after { content: "[" attr(data-n) "]" }`).
 * With the numbers out of the text nodes, the find bar matches across a marker
 * (`disappeared:` in `disappeared²:`) and copied sutta text has no numbers.
 *
 * Covers every markup source: `span.fn` (dhammatalks.org pages),
 * `sup.footnote-reference` (Nyanadipa, library books) and
 * `span.footnote-reference` (segmented texts). Idempotent: a link that already
 * has `data-n` is left alone. Run it before the footnote bar observes the
 * links, on page load and after every content swap.
 */
export function normalise_footnote_markers(root: ParentNode = document): void {
    root.querySelectorAll(FOOTNOTE_LINK_SELECTOR).forEach(link => {
        const anchor = link as HTMLAnchorElement;
        if (anchor.hasAttribute('data-n') || !is_footnote_link(anchor)) {
            return;
        }
        const n = (anchor.textContent || '').replace(/[\[\]]/g, '').trim();
        if (n === '') {
            return;
        }
        anchor.setAttribute('data-n', n);
        anchor.textContent = '';
    });
}

/**
 * Get footnote content from the page by ID
 * Returns the HTML content of the footnote definition
 */
export function get_footnote_content(footnote_id: string): string | null {
    const footnote_element = document.getElementById(footnote_id);
    if (!footnote_element) {
        return null;
    }

    // Clone the element to manipulate it without affecting the page
    const clone = footnote_element.cloneNode(true) as HTMLElement;

    // Remove the back reference link (the ↩ or arrow that links back to the reference)
    const back_links = clone.querySelectorAll('a[href^="#fr-"]');
    back_links.forEach(link => link.remove());

    // For <li> elements, get the inner HTML
    if (clone.tagName === 'LI') {
        return clone.innerHTML;
    }

    // For <div> elements with class footnote-definition, get the inner HTML
    if (clone.classList.contains('footnote-definition')) {
        // Remove the footnote label span if present
        const label = clone.querySelector('.footnote-definition-label');
        if (label) {
            label.remove();
        }
        return clone.innerHTML;
    }

    // For <p> elements that are footnote definitions (e.g., <p id="mn36note02">2. Text...</p>)
    // Strip the leading number and period
    if (clone.tagName === 'P') {
        let html = clone.innerHTML;
        // Match and remove leading number followed by period and optional space
        // e.g., "2. In other words..." -> "In other words..."
        html = html.replace(/^\d+\.\s*/, '');
        return html;
    }

    // Default: return the full inner HTML
    return clone.innerHTML;
}

/**
 * Check if a link is a footnote reference
 * Returns the footnote ID if it is, null otherwise
 */
export function is_footnote_link(anchor: HTMLAnchorElement): string | null {
    const href = anchor.getAttribute('href') || '';

    // Check if it's an anchor link
    if (!href.startsWith('#')) {
        return null;
    }

    // Check if the link has the footnote-reference class
    // or if it's inside a span with class "fn" (e.g., <span class="fn"><a href="#mn36note02">2</a></span>)
    const parent = anchor.closest('.footnote-reference, .fn');
    if (!parent) {
        return null;
    }

    // Extract the footnote ID from the href (remove the #)
    const footnote_id = href.substring(1);

    // Verify that a corresponding footnote definition exists
    const footnote_element = document.getElementById(footnote_id);
    if (!footnote_element) {
        return null;
    }

    return footnote_id;
}

/**
 * Show a footnote in a pop-up modal
 * This is called from the link click handler when a footnote reference is clicked
 */
export function show_footnote(anchor: HTMLAnchorElement): boolean {
    const footnote_id = is_footnote_link(anchor);
    if (!footnote_id) {
        return false;
    }

    const footnote_content = get_footnote_content(footnote_id);
    if (!footnote_content) {
        console.error('[FootnoteModal] Could not find footnote content for:', footnote_id);
        return false;
    }

    const footnote_number = extract_footnote_number(anchor);

    footnote_modal.show(footnote_number, footnote_content);
    return true;
}
