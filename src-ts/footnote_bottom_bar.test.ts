/**
 * Footnote markers and the footnote bottom bar, on the three markup sources
 * the reader shows: dhammatalks.org pages (`span.fn` with `<p id>` notes),
 * Nyanadipa pages and library books (`sup.footnote-reference` with `[1]`
 * text), and segmented texts (`span.footnote-reference` with an `<ol>` of
 * notes after the text).
 */

import {
  extract_footnote_number,
  get_footnote_content,
  normalise_footnote_markers,
} from "./footnote_modal";
import { footnote_bottom_bar } from "./footnote_bottom_bar";

const DHAMMATALKS_PAGE = `
  <div id="ssp_content">
    <p>it disappeared<span class="fn"><a href="#snp4-11note02">2</a></span>: and</p>
    <p id="snp4-11note02">2. A note on <a href="ssp://suttas/sn36.11/en/thanissaro">SN 36:11</a>.</p>
  </div>`;

const BOOK_PAGE = `
  <div id="ssp_content">
    <p>text<sup class="footnote-reference" id="fr-fn1-1"><a href="#fn-fn1">[1]</a></sup> more</p>
    <ol><li id="fn-fn1">The book note. <a href="#fr-fn1-1">↩</a></li></ol>
  </div>`;

const SEGMENTED_PAGE = `
  <div id="ssp_content">
    <div class="suttacentral bilara-text">
      <p>one<span class='footnote-reference'><a href='#fn-mn1-1' id='fr-mn1-1'>1</a></span>
      two<span class='footnote-reference'><a href='#fn-mn1-2' id='fr-mn1-2'>2</a></span></p>
      <section class='ssp-footnotes'><h2>Notes</h2><ol>
        <li id='fn-mn1-1' value='1' class='ssp-fn-def'>First note.</li>
        <li id='fn-mn1-2' value='2' class='ssp-fn-def'>Second <a href='ssp://suttas/mn2/pli/ms'>MN 2</a>.</li>
      </ol></section>
    </div>
  </div>`;

/** A controllable IntersectionObserver: tests call `fire()` with entries. */
class MockIntersectionObserver {
  static instances: MockIntersectionObserver[] = [];
  observed: Element[] = [];
  constructor(public callback: IntersectionObserverCallback) {
    MockIntersectionObserver.instances.push(this);
  }
  observe(el: Element) { this.observed.push(el); }
  unobserve() {}
  disconnect() { this.observed = []; }
  takeRecords() { return []; }
  fire(changes: [Element, boolean][]) {
    const entries = changes.map(([target, isIntersecting]) => ({ target, isIntersecting } as IntersectionObserverEntry));
    this.callback(entries, this as unknown as IntersectionObserver);
  }
}

function current_observer(): MockIntersectionObserver {
  return MockIntersectionObserver.instances[MockIntersectionObserver.instances.length - 1];
}

function setup_page(content: string, show_bottom_footnotes = true) {
  footnote_bottom_bar.destroy();
  document.body.innerHTML = content + `<div id="footnoteBottomBar" class="footnote-bottom-bar"></div>`;
  (document as any).SSP = { show_bottom_footnotes };
  normalise_footnote_markers(document);
  footnote_bottom_bar.init();
}

function bar(): HTMLElement {
  return document.getElementById("footnoteBottomBar")!;
}

beforeAll(() => {
  (window as any).IntersectionObserver = MockIntersectionObserver;
});

describe("normalise_footnote_markers", () => {
  test.each([
    ["span.fn", DHAMMATALKS_PAGE, ".fn a", "2"],
    ["sup.footnote-reference", BOOK_PAGE, ".footnote-reference a", "1"],
    ["span.footnote-reference", SEGMENTED_PAGE, ".footnote-reference a", "1"],
  ])("%s: number moves into data-n, text emptied", (_name, page, selector, n) => {
    document.body.innerHTML = page;
    normalise_footnote_markers(document);
    const a = document.querySelector(selector) as HTMLAnchorElement;
    expect(a.getAttribute("data-n")).toBe(n);
    expect(a.textContent).toBe("");
    expect(extract_footnote_number(a)).toBe(n);
  });

  test("is idempotent", () => {
    document.body.innerHTML = BOOK_PAGE;
    normalise_footnote_markers(document);
    normalise_footnote_markers(document);
    const a = document.querySelector(".footnote-reference a") as HTMLAnchorElement;
    expect(a.getAttribute("data-n")).toBe("1");
  });

  test("the find text runs across a marker", () => {
    document.body.innerHTML = DHAMMATALKS_PAGE;
    normalise_footnote_markers(document);
    expect(document.querySelector("#ssp_content p")!.textContent).toBe("it disappeared: and");
  });

  test("leaves a link with no note definition alone", () => {
    document.body.innerHTML = `<span class="fn"><a href="#nowhere">3</a></span>`;
    normalise_footnote_markers(document);
    const a = document.querySelector(".fn a") as HTMLAnchorElement;
    expect(a.hasAttribute("data-n")).toBe(false);
    expect(a.textContent).toBe("3");
  });
});

describe("get_footnote_content", () => {
  test("returns a list note without its back-link", () => {
    document.body.innerHTML = BOOK_PAGE;
    expect(get_footnote_content("fn-fn1")!.trim()).toBe("The book note.");
  });

  test("returns a paragraph note without its leading number", () => {
    document.body.innerHTML = DHAMMATALKS_PAGE;
    expect(get_footnote_content("snp4-11note02")).toContain("A note on");
    expect(get_footnote_content("snp4-11note02")).not.toMatch(/^2\./);
  });
});

describe("footnote bottom bar", () => {
  test("shows a note while its marker is in view, in number order", () => {
    setup_page(SEGMENTED_PAGE);
    const [m1, m2] = current_observer().observed;
    expect(current_observer().observed.length).toBe(2);

    current_observer().fire([[m2, true]]);
    current_observer().fire([[m1, true]]);
    const items = bar().querySelectorAll(".footnote-item");
    expect(Array.from(items).map(i => i.querySelector(".footnote-number")!.textContent)).toEqual(["[1]", "[2]"]);
    expect(bar().classList.contains("show")).toBe(true);

    current_observer().fire([[m1, false], [m2, false]]);
    expect(bar().querySelectorAll(".footnote-item").length).toBe(0);
    expect(bar().classList.contains("show")).toBe(false);
  });

  test("adding and removing a note leaves the other item's node in place", () => {
    setup_page(SEGMENTED_PAGE);
    const [m1, m2] = current_observer().observed;

    current_observer().fire([[m1, true]]);
    const first_item = bar().querySelector(".footnote-item")!;
    const first_text = first_item.querySelector(".footnote-content")!.firstChild;

    current_observer().fire([[m2, true]]);
    expect(bar().querySelector(".footnote-item")).toBe(first_item);
    expect(first_item.querySelector(".footnote-content")!.firstChild).toBe(first_text);

    current_observer().fire([[m2, false]]);
    expect(bar().querySelector(".footnote-item")).toBe(first_item);
    expect(first_item.isConnected).toBe(true);
  });

  test("links in a new item get the click handler once", () => {
    const add_listener = jest.spyOn(HTMLAnchorElement.prototype, "addEventListener");
    setup_page(SEGMENTED_PAGE);
    const [m1, m2] = current_observer().observed;

    current_observer().fire([[m2, true]]);
    const link = bar().querySelector(".footnote-content a")!;
    const clicks_on_link = () => add_listener.mock.contexts.filter(c => c === link).length;
    expect(clicks_on_link()).toBe(1);

    current_observer().fire([[m1, true]]);
    expect(clicks_on_link()).toBe(1);
    add_listener.mockRestore();
  });

  test("works on dhammatalks.org and book markup", () => {
    setup_page(DHAMMATALKS_PAGE);
    current_observer().fire([[current_observer().observed[0], true]]);
    expect(bar().querySelector(".footnote-number")!.textContent).toBe("[2]");
    expect(bar().querySelector(".footnote-content")!.textContent).toContain("A note on SN 36:11");

    setup_page(BOOK_PAGE);
    current_observer().fire([[current_observer().observed[0], true]]);
    expect(bar().querySelector(".footnote-number")!.textContent).toBe("[1]");
    expect(bar().querySelector(".footnote-content")!.innerHTML).not.toContain("#fr-fn1-1");
  });

  test("stays hidden when the setting is off", () => {
    setup_page(SEGMENTED_PAGE, false);
    current_observer().fire([[current_observer().observed[0], true]]);
    expect(bar().classList.contains("show")).toBe(false);
  });

  test("shows the column label when several columns have notes", () => {
    setup_page(`
      <div id="ssp_content"><div class="suttacentral bilara-text">
        <span class='footnote-reference'><a href='#fn-a-c0-1'>1</a></span>
        <span class='footnote-reference'><a href='#fn-b-c1-1'>1</a></span>
        <section class='ssp-footnotes' data-label='Bodhi'><h2>Notes: Bodhi</h2><ol><li id='fn-a-c0-1'>A</li></ol></section>
        <section class='ssp-footnotes' data-label='Sujato'><h2>Notes: Sujato</h2><ol><li id='fn-b-c1-1'>B</li></ol></section>
      </div></div>`);
    const [a, b] = current_observer().observed;
    current_observer().fire([[b, true], [a, true]]);
    const numbers = Array.from(bar().querySelectorAll(".footnote-number")).map(n => n.textContent);
    expect(numbers).toEqual(["Bodhi[1]", "Sujato[1]"]);
  });

  test("orders items by column position, not by label", () => {
    setup_page(`
      <div id="ssp_content"><div class="suttacentral bilara-text">
        <span class='footnote-reference'><a href='#fn-a-c0-2'>2</a></span>
        <span class='footnote-reference'><a href='#fn-b-c1-1'>1</a></span>
        <section class='ssp-footnotes' data-label='Sujato'><h2>Notes: Sujato</h2><ol><li id='fn-a-c0-2' value='2'>A</li></ol></section>
        <section class='ssp-footnotes' data-label='Bodhi'><h2>Notes: Bodhi</h2><ol><li id='fn-b-c1-1'>B</li></ol></section>
      </div></div>`);
    const [a, b] = current_observer().observed;
    current_observer().fire([[b, true], [a, true]]);
    const numbers = Array.from(bar().querySelectorAll(".footnote-number")).map(n => n.textContent);
    expect(numbers).toEqual(["Sujato[2]", "Bodhi[1]"]);
  });
});

describe("footnote bar toggle", () => {
  function toggle(): HTMLButtonElement {
    return document.getElementById("footnoteBarToggle") as HTMLButtonElement;
  }

  test("appears with the bar and hides it, staying visible to show it again", () => {
    setup_page(SEGMENTED_PAGE);
    expect(toggle().classList.contains("show")).toBe(false);

    current_observer().fire([[current_observer().observed[0], true]]);
    expect(bar().classList.contains("show")).toBe(true);
    expect(toggle().classList.contains("show")).toBe(true);
    expect(toggle().getAttribute("aria-label")).toBe("Hide notes");

    toggle().click();
    expect(bar().classList.contains("show")).toBe(false);
    expect(document.body.classList.contains("footnote-bar-visible")).toBe(false);
    expect(toggle().classList.contains("show")).toBe(true);
    expect(toggle().classList.contains("collapsed")).toBe(true);
    expect(toggle().getAttribute("aria-label")).toBe("Show notes");
    expect(document.body.style.getPropertyValue("--footnote-bar-height")).toBe("0px");

    toggle().click();
    expect(bar().classList.contains("show")).toBe(true);
    expect(toggle().classList.contains("collapsed")).toBe(false);
  });

  test("keeps the notes updating while hidden, without showing the bar", () => {
    setup_page(SEGMENTED_PAGE);
    const [first, second] = current_observer().observed;
    current_observer().fire([[first, true]]);
    toggle().click();
    current_observer().fire([[second, true]]);
    expect(bar().classList.contains("show")).toBe(false);
    expect(bar().querySelectorAll(".footnote-item").length).toBe(2);
    toggle().click();
    expect(bar().classList.contains("show")).toBe(true);
  });

  test("goes away when no note is in view", () => {
    setup_page(SEGMENTED_PAGE);
    const [first] = current_observer().observed;
    current_observer().fire([[first, true]]);
    current_observer().fire([[first, false]]);
    expect(toggle().classList.contains("show")).toBe(false);
  });

  test("is not shown when the setting is off", () => {
    setup_page(SEGMENTED_PAGE, false);
    current_observer().fire([[current_observer().observed[0], true]]);
    expect(toggle().classList.contains("show")).toBe(false);
  });

  test("the hidden state survives a content refresh", () => {
    setup_page(SEGMENTED_PAGE);
    current_observer().fire([[current_observer().observed[0], true]]);
    toggle().click();
    footnote_bottom_bar.refresh();
    current_observer().fire([[current_observer().observed[0], true]]);
    expect(bar().classList.contains("show")).toBe(false);
    expect(toggle().classList.contains("collapsed")).toBe(true);
  });

  test("a new page starts with the bar shown", () => {
    setup_page(SEGMENTED_PAGE);
    current_observer().fire([[current_observer().observed[0], true]]);
    toggle().click();
    setup_page(SEGMENTED_PAGE);
    current_observer().fire([[current_observer().observed[0], true]]);
    expect(bar().classList.contains("show")).toBe(true);
    expect(toggle().classList.contains("collapsed")).toBe(false);
  });
});
