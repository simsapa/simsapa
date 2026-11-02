/**
 * Footnote bottom bar: shows the notes whose references are in view.
 *
 * The bar keeps one DOM item per visible note and adds or removes only the
 * items that change, so a selection the reader is making inside a note
 * survives every update that does not remove that note, and link handlers are
 * attached to each new item once. Showing the bar reflows the page, which can
 * itself bring markers into or out of view; with incremental updates that
 * costs nothing to the items already shown.
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
        if (!setting_enabled || this.visible_footnotes.size === 0) {
            this.container.classList.remove('show');
            document.body.classList.remove('footnote-bar-visible');
            return;
        }
        this.container.classList.add('show');
        document.body.classList.add('footnote-bar-visible');
    }

    /** Disconnect the observer, empty and hide the bar. */
    public destroy(): void {
        if (this.observer) {
            this.observer.disconnect();
            this.observer = null;
        }
        this.visible_footnotes.clear();
        if (this.container) {
            this.container.innerHTML = '';
            this.container.classList.remove('show');
        }
        document.body.classList.remove('footnote-bar-visible');
        this.initialized = false;
    }

    /** Re-observe all footnotes, after the page content changed. */
    public refresh(): void {
        this.destroy();
        this.init();
    }
}

export const footnote_bottom_bar = new FootnoteBottomBar();
