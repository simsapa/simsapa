/**
 * Test cases for the Find functionality
 * Prevents regressions in search behavior, especially accent folding
 */

import { FindManager } from './find';

// Mock DOM environment for testing
function createMockDOM(): void {
    // Create a minimal DOM structure
    document.body.innerHTML = `
        <div id="findContainer">
            <button id="findSearchButton"></button>
            <div id="findBar">
                <input type="text" id="findInput">
                <div id="findCounter">0/0</div>
                <div id="findError"></div>
                <button id="findPrevButton"></button>
                <button id="findNextButton"></button>
                <input type="checkbox" id="findAccentFold" checked>
                <input type="checkbox" id="findCaseSensitive">
            </div>
        </div>
        <div id="ssp_content">
            <p><em>Satipaṭṭhāna—the establishing</em> (upaṭṭhāna) <em>of mindfulness</em> (sati)<em>—is a meditative technique for training the mind to keep mindfulness firmly established in a particular frame of reference in all its activities. The term</em> sati <em>is related to the verb</em> sarati, <em>to remember or to keep in mind. It is sometimes translated as non-reactive awareness, free from agendas, simply present with whatever arises, but the formula for satipaṭṭhāna doesn’t support that translation. Non-reactive awareness is actually an aspect of equanimity, one of the mental qualities fostered in the course of satipaṭṭhāna. The activity of satipaṭṭhāna, however, definitely has a motivating agenda: the desire for awakening, which is classed not as a cause of suffering, but as part of the path to its ending (see <a href="ssp://suttas/sn51.15/en/thanissaro">SN 51:15</a>). The role of mindfulness is to keep the mind properly focused in frames of reference that will give it guidance in what present events to develop, and which ones to abandon, so as to keep it on the path. To make an analogy, awakening is like a mountain on the horizon, the destination to which you are driving a car. Mindfulness is what remembers to keep attention focused on the road to the mountain, rather than letting it stay focused on glimpses of the mountain or get distracted by other paths leading away from the road.</em></p>
        </div>
    `;
}

describe('Find functionality', () => {
    let findManager: FindManager;

    beforeEach(() => {
        // Setup DOM
        createMockDOM();
        
        // Create FindManager instance
        findManager = new FindManager();
        
        // Wait for DOM binding
        return new Promise(resolve => setTimeout(resolve, 10));
    });

    afterEach(() => {
        // Clear any highlights
        const highlights = document.querySelectorAll('.ssp-find-highlight');
        highlights.forEach(h => h.remove());
    });

    describe('Basic search functionality', () => {
        test('should find matches for "pre"', () => {
            findManager.search('pre');
            
            const highlights = document.querySelectorAll('.ssp-find-highlight');
            expect(highlights.length).toBe(2);
        });

        test('should find matches for "pres" (regression test)', () => {
            // This was failing before the accent folding fix
            findManager.search('pres');
            
            const highlights = document.querySelectorAll('.ssp-find-highlight');
            expect(highlights.length).toBe(2);
        });

        test('should find matches for "present"', () => {
            findManager.search('present');
            
            const highlights = document.querySelectorAll('.ssp-find-highlight');
            expect(highlights.length).toBe(2);
        });

        test('should not search with less than 2 characters', () => {
            findManager.search('p');
            
            const highlights = document.querySelectorAll('.ssp-find-highlight');
            expect(highlights.length).toBe(0);
        });
    });

    describe('Accent folding functionality', () => {
        test('should create correct accent-folded patterns', () => {
            // Access the private method for testing via type assertion
            const testFindManager = findManager as any;
            
            // Test basic patterns
            expect(testFindManager.createAccentFoldedPattern('pre')).toBe('p[rṛ]e');
            expect(testFindManager.createAccentFoldedPattern('pres')).toBe('p[rṛ]e[sṣś]');
            expect(testFindManager.createAccentFoldedPattern('present')).toBe('p[rṛ]e[sṣś]e[nṅñṇ][tṭ]');
        });

        test('should handle accented characters in input', () => {
            const testFindManager = findManager as any;
            
            expect(testFindManager.createAccentFoldedPattern('ā')).toBe('[aā]');
            expect(testFindManager.createAccentFoldedPattern('ṭ')).toBe('[tṭ]');
        });

        test('should not create nested character classes (regression test)', () => {
            const testFindManager = findManager as any;
            
            const pattern = testFindManager.createAccentFoldedPattern('pres');
            
            // Should not contain nested brackets like [[sś]ṣ]
            expect(pattern).not.toMatch(/\[\[[^\]]+\][^\]]*\]/);
            
            // Should be valid regex
            expect(() => new RegExp(pattern)).not.toThrow();
        });
    });

    describe('Search state management', () => {
        test('should show error for no matches', () => {
            findManager.search('nonexistentword');
            
            const errorElement = document.getElementById('findError');
            expect(errorElement?.classList.contains('show')).toBe(true);
            expect(errorElement?.textContent).toBe('No matches found');
        });

        test('should update counter correctly', () => {
            findManager.search('present');
            
            const counterElement = document.getElementById('findCounter');
            expect(counterElement?.textContent).toBe('1/2');
        });

        test('should clear highlights when searching new term', () => {
            // First search
            findManager.search('present');
            expect(document.querySelectorAll('.ssp-find-highlight')).toHaveLength(2);
            
            // Second search should clear first highlights
            findManager.search('mind');
            const highlights = document.querySelectorAll('.ssp-find-highlight');
            
            // Should only have highlights for 'mind', not 'present'
            highlights.forEach(highlight => {
                expect(highlight.textContent?.toLowerCase()).toContain('mind');
            });
        });
    });

    describe('Navigation functionality', () => {
        beforeEach(() => {
            findManager.search('present');
        });

        test('should navigate to next match', () => {
            const initialCurrent = document.querySelector('.ssp-find-highlight.current');
            expect(initialCurrent).toBeTruthy();
            
            findManager.nextMatch();
            
            const counter = document.getElementById('findCounter');
            expect(counter?.textContent).toBe('2/2'); // Should go to second match
        });

        test('should navigate to previous match', () => {
            findManager.previousMatch();
            
            const counter = document.getElementById('findCounter');
            expect(counter?.textContent).toBe('2/2');
        });
    });

    describe('Inter-word punctuation tolerance', () => {
        test('should match "non reactive" across the hyphen in "non-reactive"', () => {
            // The page has "non-reactive" (twice, one capitalized); a space in
            // the search term must match the hyphen between the words.
            findManager.search('non reactive');

            const highlights = document.querySelectorAll('.ssp-find-highlight');
            expect(highlights.length).toBeGreaterThan(0);
            const matchedText = Array.from(highlights).map(h => h.textContent).join(' ');
            expect(matchedText.toLowerCase()).toContain('non-reactive');
        });

        test('makeInterWordFlexible leaves a single word unchanged', () => {
            const testFindManager = findManager as any;
            expect(testFindManager.makeInterWordFlexible('pajahitva')).toBe('pajahitva');
        });

        test('makeInterWordFlexible replaces an inter-word space with a punctuation class', () => {
            const testFindManager = findManager as any;
            const out = testFindManager.makeInterWordFlexible('pajahitva thito');
            // The space becomes a whitespace+punctuation class, so the words
            // still match when separated by a comma in the rendered page.
            expect(new RegExp(out, 'i').test('pajahitva, thito')).toBe(true);
            expect(new RegExp(out, 'i').test('pajahitva thito')).toBe(true);
        });
    });

    describe('Iti-sandhi boundaries', () => {
        // A find query from `content_plain` has the iti sandhi split
        // (`dhārayāmi ti`, `gantuṁ ti`); the page shows the joined form.
        beforeEach(() => {
            const content = document.getElementById('ssp_content');
            if (content) {
                content.innerHTML = `
                    <p id="p1">evaṁ dhārayāmī’ti so.</p>
                    <p id="p2">evaṁ dhārayāmī”ti so.</p>
                    <p id="p3">evaṁ dhārayāmīti so.</p>
                    <p id="p4">na sukaraṁ gantun’ti vadati.</p>
                    <p id="p5">na sukaraṁ gantu’nti vadati.</p>
                    <p id="p6">na sukaraṁ gantunti vadati.</p>
                    <p id="p7">kho bhikkhave”ti so.</p>
                `;
            }
        });

        function matched_paragraphs(): string[] {
            const ids = Array.from(document.querySelectorAll('.ssp-find-highlight'))
                .map(h => (h.closest('p') as HTMLElement).id);
            return Array.from(new Set(ids));
        }

        test.each([true, false])('dhārayāmi ti finds every written form (accent folding %s)', (fold) => {
            (document.getElementById('findAccentFold') as HTMLInputElement).checked = fold;
            findManager.search('dhārayāmi ti so');
            expect(matched_paragraphs()).toEqual(['p1', 'p2', 'p3']);
        });

        test.each([true, false])('gantuṁ ti finds n’ti, ’nti and nti (accent folding %s)', (fold) => {
            (document.getElementById('findAccentFold') as HTMLInputElement).checked = fold;
            findManager.search('sukaraṁ gantuṁ ti vadati');
            expect(matched_paragraphs()).toEqual(['p4', 'p5', 'p6']);
        });

        test('a word that is not iti sandhi still needs its own letters', () => {
            findManager.search('bhikkhave ti');
            expect(matched_paragraphs()).toEqual(['p7']);
            findManager.search('dhārayāma ti');
            expect(matched_paragraphs()).toEqual([]);
        });
    });

    describe('Double character accent folding', () => {
        beforeEach(() => {
            // Add content with Pali double characters
            const content = document.getElementById('ssp_content');
            if (content) {
                content.innerHTML = `
                    <p>viññāṇaṁ sapaññattiko jaññā añjanīva</p>
                `;
            }
        });

        test('should match vinnanam to viññāṇaṁ', () => {
            findManager.search('vinnanam');
            
            const highlights = document.querySelectorAll('.ssp-find-highlight');
            expect(highlights.length).toBeGreaterThan(0);
            
            const matchedText = Array.from(highlights)
                .map(h => h.textContent)
                .join('');
            expect(matchedText.toLowerCase()).toContain('viññāṇaṁ'.toLowerCase());
        });

        test('should match sapan to sapaññattiko (partial match)', () => {
            findManager.search('sapan');
            
            const highlights = document.querySelectorAll('.ssp-find-highlight');
            expect(highlights.length).toBeGreaterThan(0);
        });

        test('should match janna to jaññā', () => {
            findManager.search('janna');
            
            const highlights = document.querySelectorAll('.ssp-find-highlight');
            expect(highlights.length).toBeGreaterThan(0);
        });

        test('should match njaniva to añjanīva (partial match)', () => {
            findManager.search('njaniva');
            
            const highlights = document.querySelectorAll('.ssp-find-highlight');
            expect(highlights.length).toBeGreaterThan(0);
        });
    });
});
