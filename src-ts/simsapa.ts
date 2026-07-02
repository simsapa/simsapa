import * as h from "./helpers";
import { findManager } from "./find";
import "./confirm_modal";
import "./footnote_modal";
import "./invalid_link_modal";
import { footnote_bottom_bar } from "./footnote_bottom_bar";
import * as display_settings from "./display_settings";
import * as content_reload from "./content_reload";

/**
 * Attach link handlers to all links within a specific element
 * This allows us to handle sutta links, external links, etc. consistently
 */
export function attach_link_handlers_to_element(element: HTMLElement): void {
    const links = element.querySelectorAll('a');
    links.forEach(link => {
        // Check if this link is a sutta link and add the class
        const suttaResult = h.extract_sutta_uid_from_link(link as HTMLAnchorElement);
        if (suttaResult) {
            link.classList.add('sutta-link');
        }
        link.addEventListener('click', h.handle_link_click);
    });
}

function attach_link_handlers(): void {
    // h.log_info('[simsapa] Attaching link handlers');

    // Check if this is a sutta page (has ssp_content div)
    const sspContent = document.getElementById('ssp_content');

    if (sspContent) {
        // Sutta page - only add handlers to links within ssp_content
        attach_link_handlers_to_element(sspContent);
    } else {
        // Not a sutta page - add handlers to all links
        const bodyElement = document.body;
        if (bodyElement) {
            attach_link_handlers_to_element(bodyElement);
        }
    }
}

// Scroll management functions
const scrollManager = {
    // Animation state
    _animationId: null as number | null,

    // Easing function for smooth animation (ease-out cubic)
    _easeOutCubic: function(t: number): number {
        return 1 - Math.pow(1 - t, 3);
    },

    // Core smooth scroll animation function
    _smoothScrollBy: function(distance: number, duration: number = 200): void {
        // Cancel any ongoing animation
        if (this._animationId !== null) {
            cancelAnimationFrame(this._animationId);
        }

        const startY = window.scrollY;
        const startTime = performance.now();

        const animateScroll = (currentTime: number) => {
            const elapsed = currentTime - startTime;
            const progress = Math.min(elapsed / duration, 1);
            const easedProgress = this._easeOutCubic(progress);

            window.scrollTo(0, startY + distance * easedProgress);

            if (progress < 1) {
                this._animationId = requestAnimationFrame(animateScroll);
            } else {
                this._animationId = null;
            }
        };

        this._animationId = requestAnimationFrame(animateScroll);
    },

    _smoothScrollTo: function(targetY: number, duration: number = 300): void {
        const distance = targetY - window.scrollY;
        this._smoothScrollBy(distance, duration);
    },

    // Scroll by a small amount (for j/k/Up/Down keys)
    scrollSmallUp: function(): void {
        this._smoothScrollBy(-80, 150);
    },
    scrollSmallDown: function(): void {
        this._smoothScrollBy(80, 150);
    },
    // Scroll by half a page (for Ctrl+U/Ctrl+D)
    scrollHalfPageUp: function(): void {
        const halfPage = window.innerHeight / 2;
        this._smoothScrollBy(-halfPage, 250);
    },
    scrollHalfPageDown: function(): void {
        const halfPage = window.innerHeight / 2;
        this._smoothScrollBy(halfPage, 250);
    },
    // Scroll by a full page (for Space/Shift+Space/Page Up/Page Down)
    scrollPageUp: function(): void {
        const page = window.innerHeight - 50; // Leave some overlap
        this._smoothScrollBy(-page, 300);
    },
    scrollPageDown: function(): void {
        const page = window.innerHeight - 50; // Leave some overlap
        this._smoothScrollBy(page, 300);
    },
    // Scroll to beginning/end (for Home/End)
    scrollToTop: function(): void {
        this._smoothScrollTo(0, 400);
    },
    scrollToBottom: function(): void {
        const maxScroll = document.documentElement.scrollHeight - window.innerHeight;
        this._smoothScrollTo(maxScroll, 400);
    },
};

document.SSP = {
    show_transient_message: h.show_transient_message,
    find: findManager,
    attach_link_handlers: attach_link_handlers,
    show_bottom_footnotes: true, // Default to true, will be updated from QML
    scroll: scrollManager,
};

/**
 * Refresh the footnote bottom bar based on current settings
 * Called from QML when the show_bottom_footnotes setting changes
 */
function footnote_bottom_bar_refresh(): void {
    if (!document.SSP.show_bottom_footnotes) {
        // Setting is disabled, destroy the footnote bar
        footnote_bottom_bar.destroy();
    } else {
        // Setting is enabled, refresh the footnote bar
        footnote_bottom_bar.refresh();
    }
}

// Expose to window for QML access
(window as any).footnote_bottom_bar_refresh = footnote_bottom_bar_refresh;

document.addEventListener('DOMContentLoaded', () => {
    // h.log_info('[simsapa] DOMContentLoaded event fired');
    attach_link_handlers();

    // Initialize footnote bottom bar for sutta pages if enabled
    const sspContent = document.getElementById('ssp_content');
    if (sspContent && document.SSP.show_bottom_footnotes) {
        footnote_bottom_bar.init();
    }

    // Display-settings cogwheel panel: only present on sutta pages
    // (init is a no-op when the chrome is absent). Layout changes re-render
    // the content block through the localhost API.
    if (document.getElementById('displaySettingsButton')) {
        display_settings.set_layout_change_handler((layout) => {
            content_reload.refetch_with_layout(layout);
        });
        display_settings.init_display_settings();
    }
});
