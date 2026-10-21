/**
 * Tests for the tap / double-click word boundaries (src-ts/pali_word.ts).
 * The quoted iti-sandhi forms are the ones that occur in the shipped texts:
 * SuttaCentral `”ti` / `’ti` / `’”ti` / `”’ti`, CST `’’ti` / `‘‘ti` / `'nti`.
 */

import { word_at_offset, expand_selection_to_word } from "./pali_word";

/** All words of `text`, in order, as the tap handler would find them. */
function words(text: string): string[] {
  const found: string[] = [];
  let offset = 0;
  while (offset <= text.length) {
    const span = word_at_offset(text, offset);
    if (span.start < 0) {
      offset += 1;
      continue;
    }
    if (found.length === 0 || found[found.length - 1] !== span.word) {
      found.push(span.word);
    }
    offset = span.end + 1;
  }
  return found;
}

describe("word_at_offset", () => {
  test.each([
    ["lābhanānattan”ti.", "lābhanānattan”ti"],
    ["dassanakāmo’”ti?", "dassanakāmo’”ti"],
    ["sassatisaman”’ti", "sassatisaman”’ti"],
    ["dhārayāmī”ti", "dhārayāmī”ti"],
    ["dhārayāmī’”ti", "dhārayāmī’”ti"],
    ['dhārayāmī"ti', 'dhārayāmī"ti'],
    ["dhārayāmī'\"ti", "dhārayāmī'\"ti"],
    ["dhārayāmī’ti", "dhārayāmī’ti"],
    ["karissāmā’’ti", "karissāmā’’ti"],
    ["kimaṁsisū‘‘ti", "kimaṁsisū‘‘ti"],
    ["sīmavisodhani'nti", "sīmavisodhani'nti"],
    ["cā”tipi", "cā”tipi"],
    ["sampayuttan”tipi,", "sampayuttan”tipi"],
    ["āvuso”tissa", "āvuso”tissa"],
  ])("%s is one word", (text, word) => {
    expect(words(text)).toEqual([word]);
  });

  test("a tap on the ti half finds the whole word", () => {
    const text = "“lābhanānattan”ti.";
    const span = word_at_offset(text, text.indexOf("ti"));
    expect(span).toEqual({ word: "lābhanānattan”ti", start: 1, end: 17 });
  });

  test.each([
    ["bhikkhave” tiṭṭhati", ["bhikkhave", "tiṭṭhati"]],
    ["manopubbaṅ'gamā dhammā", ["manopubbaṅ'gamā", "dhammā"]],
    ["“Ehi, bhikkhu”", ["Ehi", "bhikkhu"]],
    ["“evaṁ, bhante”ti, “tiṭṭhatu”", ["evaṁ", "bhante”ti", "tiṭṭhatu"]],
  ])("%s keeps its other boundaries", (text, expected) => {
    expect(words(text)).toEqual(expected);
  });

  test("no word at the offset", () => {
    expect(word_at_offset("— ”", 1)).toEqual({ word: "", start: -1, end: -1 });
  });
});

describe("expand_selection_to_word", () => {
  function select(node: Text, start: number, end: number): Selection {
    const selection = document.getSelection()!;
    selection.setBaseAndExtent(node, start, node, end);
    return selection;
  }

  beforeEach(() => {
    document.body.innerHTML = "<p>“Ayaṁ lābhanānattan”ti. Evaṁ.</p>";
  });

  function text_node(): Text {
    return document.querySelector("p")!.firstChild as Text;
  }

  test.each([
    ["the word before the quote", "lābhanānattan"],
    ["the ti after the quote", "ti"],
  ])("widens a double-click on %s", (_label, part) => {
    const node = text_node();
    const value = node.nodeValue!;
    const start = value.indexOf(part, value.indexOf("lābh"));
    const selection = select(node, start, start + part.length);

    expand_selection_to_word();

    expect(selection.toString()).toBe("lābhanānattan”ti");
  });

  test("widens a Windows double-click that took the trailing space", () => {
    document.body.innerHTML = "<p>Sādhu, bhante”ti so bhikkhu.</p>";
    const node = text_node();
    const value = node.nodeValue!;
    const start = value.indexOf("ti so");
    const selection = select(node, start, start + "ti ".length);

    expand_selection_to_word();

    expect(selection.toString()).toBe("bhante”ti");
  });

  test("leaves an ordinary word as the browser selected it", () => {
    const node = text_node();
    const start = node.nodeValue!.indexOf("Ayaṁ");
    const selection = select(node, start, start + 4);

    expand_selection_to_word();

    expect(selection.toString()).toBe("Ayaṁ");
  });

  test("never narrows a longer selection", () => {
    const node = text_node();
    const value = node.nodeValue!;
    const selection = select(node, value.indexOf("Ayaṁ"), value.indexOf("Evaṁ"));
    const before = selection.toString();

    expand_selection_to_word();

    expect(selection.toString()).toBe(before);
  });
});
