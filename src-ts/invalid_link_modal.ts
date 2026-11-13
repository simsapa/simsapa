/**
 * Invalid link modal for displaying invalid link warnings
 * Shows a warning when clicking on links that are not valid routes
 */

import { attach_link_handlers_to_element } from "./simsapa";

class InvalidLinkModal {
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
        const overlay = document.getElementById('invalidLinkModalOverlay');
        const title_element = document.getElementById('invalidLinkModalTitle');
        const body_element = document.getElementById('invalidLinkModalBody');
        const close_button = document.getElementById('invalidLinkModalClose');

        if (!overlay || !title_element || !body_element || !close_button) {
            console.error('[InvalidLinkModal] Required elements not found in DOM');
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
     * Show the invalid link modal with the invalid URL
     */
    public show(url: string): void {
        // Always try to initialize (will skip if already initialized with valid DOM)
        this.init();

        if (!this.overlay || !this.title_element || !this.body_element) {
            console.error('[InvalidLinkModal] Modal not initialized');
            return;
        }

        // Set the title
        this.title_element.textContent = 'Invalid Link';

        // Set the body content
        this.body_element.innerHTML = `Invalid link target: <code>${url}</code>`;

        // Attach link handlers to links within the modal body (though unlikely)
        attach_link_handlers_to_element(this.body_element);

        // Show the modal
        this.overlay.classList.add('show');
    }

    /**
     * Show the modal with a plain-text message, e.g. the Gloss Selection
     * action's "no Pāli text selected" notice. The text is not parsed as HTML.
     */
    public show_message(title: string, message: string): void {
        this.init();

        if (!this.overlay || !this.title_element || !this.body_element) {
            console.error('[InvalidLinkModal] Modal not initialized');
            return;
        }

        this.title_element.textContent = title;
        const p = document.createElement('p');
        p.className = 'invalid-link-modal-content ssp-message-text';
        p.textContent = message;
        this.body_element.replaceChildren(p);

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
     * Close the invalid link modal
     * Public so it can be called from other modules
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
export const invalid_link_modal = new InvalidLinkModal();