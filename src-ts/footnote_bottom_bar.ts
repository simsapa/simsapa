/**
 * Footnote bottom bar: shows the notes whose references are in view.
 *
 * The bar keeps one DOM item per visible note and adds or removes only the
 * items that change, so a selection the reader is making inside a note
 * survives every update that does not remove that note, and link handlers are
 * attached to each new item once. Showing the bar reflows the page, which can
 * itself bring markers into or out of view; with incremental updates that
 * costs nothing to the items already shown.
 *
 * A round toggle button sits above the bar's top edge while the bar has notes
 * to show. It hides the bar for the rest of the page's life (the setting is
 * not changed) and stays, lowered to the bottom edge, to bring the bar back.
 * Its offset follows the bar's measured height through the
 * `--footnote-bar-height` property on `<body>`.
 */

import { FOOTNOTE_LINK_SELECTOR, extract_footnote_number, get_footnote_content, is_footnote_link } from "./footnote_modal";
import { attach_link_handlers_to_element } from "./simsapa";

interface VisibleFootnote {
    id: string;
    number: string;
    /** The column label, for pages where several columns have notes. */
    label: string;
    /** The column's position on the page (0 on a single-text page). */
    column: number;
    element: HTMLElement;
}

/**
 * The label of the column a note belongs to: the `data-label` of its notes
 * list (aligned multi-column view), or the header of its block column
 * (unaligned fallback). Empty on a single-text page.
 */
function column_label_of(footnote_id: string): string {
    const definition = document.getElementById(footnote_id);
    if (!definition) {
        return '';
    }
    const section = definition.closest('section.ssp-footnotes') as HTMLElement | null;
    if (section && section.dataset.label) {
        return section.dataset.label;
    }
    const header = definition.closest('.sbs-col')?.querySelector('.sbs-col-header');
    return header?.textContent?.trim() || '';
}

/**
 * The position of the column a note belongs to, in page order: its notes
 * list among the page's notes lists (aligned multi-column view, one list per
 * column in column order), or its block column among the block columns
 * (unaligned fallback). 0 on a single-text page.
 */
function column_index_of(footnote_id: string): number {
    const definition = document.getElementById(footnote_id);
    if (!definition) {
        return 0;
    }
    const section = definition.closest('section.ssp-footnotes');
    if (section) {
        const index = Array.from(document.querySelectorAll('section.ssp-footnotes')).indexOf(section);
        if (index >= 0) {
            return index;
        }
    }
    const col = definition.closest('.sbs-col');
    if (col) {
        return Math.max(0, Array.from(document.querySelectorAll('.sbs-col')).indexOf(col));
    }
    return 0;
}

/** Whether notes come from more than one column, so items need the label. */
function several_columns_have_notes(): boolean {
    const labels = new Set<string>();
    document.querySelectorAll('section.ssp-footnotes').forEach(section => {
        labels.add((section as HTMLElement).dataset.label || '');
    });
    document.querySelectorAll('.sbs-col').forEach(col => {
        if (col.querySelector(FOOTNOTE_LINK_SELECTOR)) {
            labels.add(col.querySelector('.sbs-col-header')?.textContent?.trim() || '');
        }
    });
    return labels.size > 1;
}

// mdi eye-off-outline / eye-outline (assets/icons/svg/mdi--eye-*.svg).
const HIDE_ICON = '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M2 5.27L3.28 4L20 20.72L18.73 22l-3.08-3.08c-1.15.38-2.37.58-3.65.58c-5 0-9.27-3.11-11-7.5c.69-1.76 1.79-3.31 3.19-4.54zM12 9a3 3 0 0 1 3 3a3 3 0 0 1-.17 1L11 9.17A3 3 0 0 1 12 9m0-4.5c5 0 9.27 3.11 11 7.5a11.8 11.8 0 0 1-4 5.19l-1.42-1.43A9.86 9.86 0 0 0 20.82 12A9.82 9.82 0 0 0 12 6.5c-1.09 0-2.16.18-3.16.5L7.3 5.47c1.44-.62 3.03-.97 4.7-.97M3.18 12A9.82 9.82 0 0 0 12 17.5c.69 0 1.37-.07 2-.21L11.72 15A3.064 3.064 0 0 1 9 12.28L5.6 8.87c-.99.85-1.82 1.91-2.42 3.13"/></svg>';
const SHOW_ICON = '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M12 9a3 3 0 0 1 3 3a3 3 0 0 1-3 3a3 3 0 0 1-3-3a3 3 0 0 1 3-3m0-4.5c5 0 9.27 3.11 11 7.5c-1.73 4.39-6 7.5-11 7.5S2.73 16.39 1 12c1.73-4.39 6-7.5 11-7.5M3.18 12a9.821 9.821 0 0 0 17.64 0a9.821 9.821 0 0 0-17.64 0"/></svg>';

const TOGGLE_ID = 'footnoteBarToggle';

/** Column order first (each column numbers its own notes), then number. */
function numeric_order(a: VisibleFootnote, b: VisibleFootnote): number {
    if (a.column !== b.column) {
        return a.column - b.column;
    }
    const num_a = parseInt(a.number, 10);
    const num_b = parseInt(b.number, 10);
    if (!isNaN(num_a) && !isNaN(num_b)) {
        return num_a - num_b;
    }
    return a.number.localeCompare(b.number);
}

class FootnoteBottomBar {
    private container: HTMLElement | null = null;
    private observer: IntersectionObserver | null = null;
    private visible_footnotes: Map<string, VisibleFootnote> = new Map();
    private show_labels: boolean = false;
    private initialized: boolean = false;
    private toggle: HTMLButtonElement | null = null;
    private resize_observer: ResizeObserver | null = null;
    /** Hidden with the toggle; kept across refresh() for the page's life. */
    private user_hidden: boolean = false;

    /**
     * Find the container, observe every footnote reference on the page.
     * Run `normalise_footnote_markers()` first.
     */
    public init(): void {
        if (this.initialized) {
            return;
        }

        this.container = document.getElementById('footnoteBottomBar');
        if (!this.container) {
            console.error('[FootnoteBottomBar] Container element not found in DOM');
            return;
        }
        this.container.innerHTML = '';
        this.show_labels = several_columns_have_notes();

        this.setup_toggle();
        this.setup_observer();
        this.observe_all_footnotes();

        this.initialized = true;
    }

    private setup_observer(): void {
        const options: IntersectionObserverInit = {
            root: null,
            rootMargin: '0px',
            // A marker drawn by CSS has its box from the ::after content.
            threshold: 0.5,
        };

        this.observer = new IntersectionObserver((entries) => {
            entries.forEach(entry => {
                const anchor = entry.target as HTMLAnchorElement;
                const footnote_id = is_footnote_link(anchor);
                if (!footnote_id) {
                    return;
                }
                if (entry.isIntersecting) {
                    this.add_visible_footnote(anchor, footnote_id);
                } else {
                    this.remove_visible_footnote(footnote_id);
                }
            });
            this.update_visibility();
        }, options);
    }

    /** Find or create the toggle button; follow the bar's height. */
    private setup_toggle(): void {
        let toggle = document.getElementById(TOGGLE_ID) as HTMLButtonElement | null;
        if (!toggle) {
            toggle = document.createElement('button');
            toggle.id = TOGGLE_ID;
            toggle.type = 'button';
            toggle.className = 'footnote-bar-toggle';
            // Not the reader's text: the page's tap and double-click word
            // lookup must not see this click.
            toggle.addEventListener('click', (event) => {
                event.preventDefault();
                event.stopPropagation();
                this.set_user_hidden(!this.user_hidden);
            });
            // Toggling twice quickly is a double-click on desktop.
            toggle.addEventListener('dblclick', (event) => {
                event.preventDefault();
                event.stopPropagation();
            });
            document.body.appendChild(toggle);
            // The button lives outside #ssp_content, so it survives content
            // swaps; a new one means a new page, which starts with the bar shown.
            this.user_hidden = false;
        }
        this.toggle = toggle;

        if (typeof ResizeObserver !== 'undefined' && this.container) {
            this.resize_observer = new ResizeObserver(() => this.update_bar_height());
            this.resize_observer.observe(this.container);
        }
    }

    private set_user_hidden(hidden: boolean): void {
        this.user_hidden = hidden;
        this.update_visibility();
    }

    /** Publish the bar's height for the toggle's offset (0 when hidden). */
    private update_bar_height(): void {
        const shown = this.container?.classList.contains('show') ?? false;
        const height = shown && this.container ? this.container.offsetHeight : 0;
        document.body.style.setProperty('--footnote-bar-height', `${height}px`);
    }

    private update_toggle(has_notes: boolean): void {
        if (!this.toggle) {
            return;
        }
        this.toggle.classList.toggle('show', has_notes);
        // Runs on every observer callback: rewrite the icon only on a change,
        // so a tap in progress keeps its target.
        const state = this.user_hidden ? 'hidden' : 'shown';
        if (this.toggle.dataset.state === state) {
            return;
        }
        this.toggle.dataset.state = state;
        this.toggle.classList.toggle('collapsed', this.user_hidden);
        this.toggle.innerHTML = this.user_hidden ? SHOW_ICON : HIDE_ICON;
        const label = this.user_hidden ? 'Show notes' : 'Hide notes';
        this.toggle.title = label;
        this.toggle.setAttribute('aria-label', label);
        this.toggle.setAttribute('aria-pressed', this.user_hidden ? 'true' : 'false');
    }

    private observe_all_footnotes(): void {
        if (!this.observer) {
            return;
        }
        document.querySelectorAll(FOOTNOTE_LINK_SELECTOR).forEach(link => {
            const anchor = link as HTMLAnchorElement;
            if (is_footnote_link(anchor)) {
                this.observer!.observe(anchor);
            }
        });
    }

    private build_item(footnote: Omit<VisibleFootnote, 'element'>, content: string): HTMLElement {
        const item = document.createElement('div');
        item.className = 'footnote-item';
        if (document.getElementById(footnote.id)?.classList.contains('ssp-fn-editor')) {
            item.classList.add('ssp-fn-editor');
        }
        item.dataset.footnoteId = footnote.id;

        const number = document.createElement('span');
        number.className = 'footnote-number';
        if (this.show_labels && footnote.label) {
            const label = document.createElement('span');
            label.className = 'footnote-column-label';
            label.textContent = footnote.label;
            number.appendChild(label);
        }
        number.appendChild(document.createTextNode(`[${footnote.number}]`));

        const body = document.createElement('div');
        body.className = 'footnote-content';
        body.innerHTML = content;

        item.appendChild(number);
        item.appendChild(body);

        attach_link_handlers_to_element(body);
        return item;
    }

    /** Insert a note's item in order, leaving the other items untouched. */
    private add_visible_footnote(anchor: HTMLAnchorElement, footnote_id: string): void {
        if (!this.container || this.visible_footnotes.has(footnote_id)) {
            return;
        }
        const content = get_footnote_content(footnote_id);
        if (!content) {
            return;
        }

        const info = {
            id: footnote_id,
            number: extract_footnote_number(anchor),
            label: column_label_of(footnote_id),
            column: column_index_of(footnote_id),
        };
        const footnote: VisibleFootnote = { ...info, element: this.build_item(info, content) };

        const next = Array.from(this.visible_footnotes.values())
            .filter(other => numeric_order(footnote, other) < 0)
            .sort(numeric_order)[0];
        this.container.insertBefore(footnote.element, next ? next.element : null);
        this.visible_footnotes.set(footnote_id, footnote);
    }

    /** Remove one note's item, leaving the other items untouched. */
    private remove_visible_footnote(footnote_id: string): void {
        const footnote = this.visible_footnotes.get(footnote_id);
        if (!footnote) {
            return;
        }
        footnote.element.remove();
        this.visible_footnotes.delete(footnote_id);
    }

    /** Show or hide the bar; never rebuilds its items. */
    private update_visibility(): void {
        if (!this.container) {
            return;
        }
        const setting_enabled = (document as any).SSP?.show_bottom_footnotes !== false;
        const has_notes = setting_enabled && this.visible_footnotes.size > 0;
        const show = has_notes && !this.user_hidden;
        this.container.classList.toggle('show', show);
        document.body.classList.toggle('footnote-bar-visible', show);
        this.update_toggle(has_notes);
        this.update_bar_height();
    }

    /** Disconnect the observer, empty and hide the bar. */
    public destroy(): void {
        if (this.observer) {
            this.observer.disconnect();
            this.observer = null;
        }
        if (this.resize_observer) {
            this.resize_observer.disconnect();
            this.resize_observer = null;
        }
        this.visible_footnotes.clear();
        if (this.container) {
            this.container.innerHTML = '';
            this.container.classList.remove('show');
        }
        this.toggle?.classList.remove('show');
        document.body.classList.remove('footnote-bar-visible');
        document.body.style.removeProperty('--footnote-bar-height');
        this.initialized = false;
    }

    /** Re-observe all footnotes, after the page content changed. */
    public refresh(): void {
        this.destroy();
        this.init();
    }
}

export const footnote_bottom_bar = new FootnoteBottomBar();
