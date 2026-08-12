import { candidate_ids, jump_to_segment, show_anchor_notice } from "./anchor_jump";

// jsdom has no scrollTo; without a stub the give-up path logs a
// "not implemented" trace on every test that reaches it.
beforeEach(() => {
    (window as any).scrollTo = jest.fn();
});

function set_content(inner: string): HTMLElement {
    document.body.innerHTML = `<div id="ssp_content">${inner}</div>`;
    return document.getElementById("ssp_content") as HTMLElement;
}

describe("candidate_ids", () => {
    test("decrements the last component to zero, then the parent, then stops", () => {
        const ids = candidate_ids("dn33:1.7.9.10");

        expect(ids[0]).toBe("dn33:1.7.9.10");
        expect(ids[1]).toBe("dn33:1.7.9.9");
        expect(ids[10]).toBe("dn33:1.7.9.0");
        expect(ids[11]).toBe("dn33:1.7.9");
        expect(ids.length).toBe(12);

        // Never a sibling of the parent — that may be an unrelated chapter.
        expect(ids).not.toContain("dn33:1.7.8");
        expect(ids).not.toContain("dn33:1.7");
        expect(ids).not.toContain("dn33:1");
    });

    test("the measured failing locations resolve within one decrement step", () => {
        expect(candidate_ids("dn33:1.7.9.1")).toEqual([
            "dn33:1.7.9.1",
            "dn33:1.7.9.0",
            "dn33:1.7.9",
        ]);
        expect(candidate_ids("dn20:4.15").slice(0, 6)).toEqual([
            "dn20:4.15",
            "dn20:4.14",
            "dn20:4.13",
            "dn20:4.12",
            "dn20:4.11",
            "dn20:4.10",
        ]);
    });

    test("a non-numeric last component skips the decrements but keeps the parent", () => {
        expect(candidate_ids("dn33:1.7.end")).toEqual(["dn33:1.7.end", "dn33:1.7"]);
    });

    test("an id with no dots and no colon neither crashes nor loops", () => {
        expect(candidate_ids("intro")).toEqual(["intro"]);
        expect(candidate_ids("dn33:0")).toEqual(["dn33:0"]);
        expect(candidate_ids("")).toEqual([""]);
    });
});

describe("jump_to_segment", () => {
    test("an exact hit scrolls, highlights, and shows no notice", () => {
        set_content(`<p><span class="segment" id="dn33:1.11.0">Evaṁ me sutaṁ</span></p>`);

        expect(jump_to_segment("dn33:1.11.0")).toBe("exact");
        expect(document.querySelectorAll(".ssp-anchor-notice").length).toBe(0);
        const el = document.getElementById("dn33:1.11.0") as HTMLElement;
        expect(el.classList.contains("ssp-anchor-highlight")).toBe(true);
        expect(el.scrollIntoView).toHaveBeenCalled();
    });

    test("a missing segment falls back to the nearest preceding sibling", () => {
        set_content(`<p><span class="segment" id="dn33:1.7.9.0">1. Ones</span></p>`);

        expect(jump_to_segment("dn33:1.7.9.1")).toBe("fallback:dn33:1.7.9.0");

        const notice = document.querySelector(".ssp-anchor-notice") as HTMLElement;
        expect(notice).not.toBeNull();
        expect(notice.textContent).toContain("Referenced location dn33:1.7.9.1 not found.");
        expect(notice.textContent).toContain("This location dn33:1.7.9.0 is the closest fallback.");
    });

    test("resolves against the parent when only the parent exists", () => {
        // Real data never exercises this branch — Bilara emits headings as
        // x.y.z.0, never as the bare parent — so it is only reachable here.
        set_content(`<p><span class="segment" id="dn33:1.7.9">parent</span></p>`);

        expect(jump_to_segment("dn33:1.7.9.2")).toBe("fallback:dn33:1.7.9");
    });

    test("never walks past the parent section", () => {
        set_content(`<p><span class="segment" id="dn33:1.7.8">another chapter</span></p>`);

        expect(jump_to_segment("dn33:1.7.9.2")).toBe("missed");
    });

    test("finds an anchor that uses name= rather than id=", () => {
        // The reason the wrappers ran a JS pass at all before this module
        // existed; losing it would turn such anchors into misses.
        set_content(`<p><a name="dn33:1.11.0">Evaṁ me sutaṁ</a></p>`);

        expect(jump_to_segment("dn33:1.11.0")).toBe("exact");
        expect(document.querySelectorAll(".ssp-anchor-notice").length).toBe(0);
    });

    test("a miss scrolls to the top and shows the one-sentence notice first in the content", () => {
        const content = set_content(`<h1>Saṅgītisutta</h1><p>legacy text, no segments</p>`);

        expect(jump_to_segment("dn20:4.11")).toBe("missed");

        expect(window.scrollTo).toHaveBeenCalledWith(0, 0);
        const notice = content.firstChild as HTMLElement;
        expect(notice.classList.contains("ssp-anchor-notice")).toBe(true);
        expect(notice.textContent).toContain("Referenced location dn20:4.11 not found.");
        expect(notice.textContent).not.toContain("closest fallback");
    });

    test("two misses in a row leave exactly one notice", () => {
        set_content(`<p>no segments here</p>`);

        jump_to_segment("dn20:4.11");
        jump_to_segment("dn20:4.12");

        expect(document.querySelectorAll(".ssp-anchor-notice").length).toBe(1);
        const notice = document.querySelector(".ssp-anchor-notice") as HTMLElement;
        expect(notice.textContent).toContain("dn20:4.12");
    });

    test("an exact hit after a miss clears the stale notice", () => {
        set_content(`<p><span class="segment" id="dn33:1.11.0">text</span></p>`);

        jump_to_segment("zz99:9.9");
        expect(document.querySelectorAll(".ssp-anchor-notice").length).toBe(1);

        jump_to_segment("dn33:1.11.0");
        expect(document.querySelectorAll(".ssp-anchor-notice").length).toBe(0);
    });
});

describe("the notice", () => {
    test("goes before the nearest block ancestor, never inside span.segment", () => {
        set_content(`<p id="para"><span class="segment" id="dn33:1.7.9.0">1. Ones</span></p>`);

        jump_to_segment("dn33:1.7.9.1");

        const notice = document.querySelector(".ssp-anchor-notice") as HTMLElement;
        const para = document.getElementById("para") as HTMLElement;
        expect(notice.nextElementSibling).toBe(para);
        expect(para.querySelector(".ssp-anchor-notice")).toBeNull();
        expect(document.querySelector("span.segment .ssp-anchor-notice")).toBeNull();
    });

    test("the fallback scroll targets the notice, not the paragraph below it", () => {
        // The notice is inserted above the resolved paragraph, so scrolling to
        // the paragraph would push the explanation off the top edge.
        set_content(`<p id="para"><span class="segment" id="dn33:1.7.9.0">1. Ones</span></p>`);
        const target = document.getElementById("dn33:1.7.9.0") as HTMLElement;
        const scrolled: HTMLElement[] = [];
        (HTMLElement.prototype.scrollIntoView as jest.Mock).mockImplementation(function (this: HTMLElement) {
            scrolled.push(this);
        });

        jump_to_segment("dn33:1.7.9.1");

        const notice = document.querySelector(".ssp-anchor-notice") as HTMLElement;
        expect(scrolled).toEqual([notice]);
        expect(scrolled).not.toContain(target);
        (HTMLElement.prototype.scrollIntoView as jest.Mock).mockReset();
    });

    test("an exact hit still scrolls to the paragraph itself", () => {
        set_content(`<p><span class="segment" id="dn33:1.11.0">text</span></p>`);
        const target = document.getElementById("dn33:1.11.0") as HTMLElement;

        jump_to_segment("dn33:1.11.0");

        expect(target.scrollIntoView).toHaveBeenCalled();
    });

    test("the message is real selectable text carrying both full ids", () => {
        set_content(`<p><span class="segment" id="dn20:4.10">text</span></p>`);

        jump_to_segment("dn20:4.15");

        const message = document.querySelector(".ssp-anchor-notice-message") as HTMLElement;
        expect(message.textContent).toBe(
            "Referenced location dn20:4.15 not found. This location dn20:4.10 is the closest fallback.");
        // Full ids, not the short form printed in the margin.
        expect(message.textContent).toContain("dn20:4.15");
        expect(message.textContent).toContain("dn20:4.10");
    });

    test("the dismiss control removes it, and still works after the find bar splices spans into the text", () => {
        set_content(`<p><span class="segment" id="dn20:4.10">text</span></p>`);
        jump_to_segment("dn20:4.15");

        const notice = document.querySelector(".ssp-anchor-notice") as HTMLElement;
        const message = notice.querySelector(".ssp-anchor-notice-message") as HTMLElement;
        // What findAndReplace() does to a matching text node.
        message.innerHTML = message.innerHTML.replace(
            "location", `<span class="ssp-find-highlight">location</span>`);

        const dismiss = notice.querySelector(".ssp-anchor-notice-dismiss") as HTMLElement;
        expect(dismiss.getAttribute("aria-label")).toBeTruthy();
        dismiss.click();

        expect(document.querySelectorAll(".ssp-anchor-notice").length).toBe(0);
    });

    test("show_anchor_notice falls back to the top of the content when it has no target", () => {
        const content = set_content(`<p>text</p>`);

        show_anchor_notice("dn20:4.11", null, null);

        expect((content.firstChild as HTMLElement).classList.contains("ssp-anchor-notice")).toBe(true);
    });
});
