/**
 * The Pāli word under a tap or a double-click, for the dictionary lookup.
 *
 * A word is a run of letters and combining marks, plus `'` and `’`, which the
 * texts use inside words (`manopubbaṅ'gamā`, `dhārayāmī’ti`, CST's
 * `karissāmā’’ti`). It also continues across any run of quote marks into an
 * iti-sandhi `ti` / `nti`, and on into the word iti fused with (`cā”tipi` is
 * ca + iti + api), because SuttaCentral writes most of them with a double
 * closing quote (`lābhanānattan”ti`, `dassanakāmo’”ti`, `sassatisaman”’ti`).
 * Without that, a tap on either half looked up a fragment: `ti` shows the
 * particle, and a fragment such as `lābhanānattan` misses its break-down or is
 * found only by a slow table scan. The backend resolves the whole word
 * whichever quotes it carries (see docs/pali-sandhi-and-quote-marks.md).
 */

const RE_PALI_WORD = /[\p{L}\p{M}'’]+(?:[‘“"”]+[‘“"”'’]*n?ti[\p{L}\p{M}'’]*)?/gu;

export interface WordSpan {
  word: string;
  start: number;
  end: number;
}

/** The word in `text` that contains `offset` (inclusive of both ends). */
export function word_at_offset(text: string, offset: number): WordSpan {
  for (const match of text.matchAll(RE_PALI_WORD)) {
    const start = match.index;
    const end = start + match[0].length;
    if (offset >= start && offset <= end) {
      return { word: match[0], start: start, end: end };
    }
  }
  return { word: "", start: -1, end: -1 };
}

/**
 * Widen the current selection to the whole Pāli word it lies in.
 *
 * The browser's double-click selects by Unicode word boundaries, which end a
 * word at `”` and `"`: double-clicking `lābhanānattan”ti` selects only
 * `lābhanānattan` or `ti`. This extends the selection to the span
 * `word_at_offset` finds. It only ever widens, and only within one text node;
 * any other selection is left as it is.
 */
export function expand_selection_to_word(): void {
  const selection = document.getSelection();
  if (!selection || selection.rangeCount !== 1) return;
  const range = selection.getRangeAt(0);
  const node = range.startContainer;
  if (node.nodeType !== Node.TEXT_NODE || range.endContainer !== node) return;

  const value = node.nodeValue || "";
  // On Windows the browser's double-click also selects the whitespace after
  // the word (`ti ` in `bhante”ti so`). Compare without it, or the selection
  // looks longer than the word and is never widened.
  let end = range.endOffset;
  while (end > range.startOffset && /\s/.test(value[end - 1])) {
    end -= 1;
  }

  const found = word_at_offset(value, range.startOffset);
  if (found.start < 0) return;
  if (found.start > range.startOffset || found.end < end) return;
  if (found.start === range.startOffset && found.end === end) return;

  selection.setBaseAndExtent(node, found.start, node, found.end);
}
