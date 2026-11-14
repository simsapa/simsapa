# Review: the cost of "Contextual meaning for every word", and the translation table

For the maintainer. Task list: 7.0 in
[2026-11-13-135127-tasks-word-in-sentence-two-line-layout-and-contextual-column.md](./2026-11-13-135127-tasks-word-in-sentence-two-line-layout-and-contextual-column.md).
PRD: FR-G7 (cost), FR-J4 (translation comparison).

## What was run, and what could not be

- Model: `gemini-3.6-flash` (Gemini), on the free-tier key in `bootstrap_cli/.env`,
  2026-11-14. `simsapa gloss --model-name gemini-3.6-flash --no-translate
  --format json`, one fresh run per passage and variant. The context cache in
  the dist `appdata.sqlite3` was empty, and every run held `cli-write.lock`, so no
  run wrote cache rows and none saw another's answers.
- The free tier allows **20 requests a day** for this model. The option-**off**
  runs used the day's quota. Every option-**on** request, the FR-G8 run without
  the `contextual_meanings` sentence, and the FR-J4 translations failed with a
  rate-limit error. The maintainer chose to go on with the data that was
  collected.
- What was still measured exactly, at no cost: the option-**on** requests' item
  count and prompt size. The CLI was run with an invalid API key. The batch is
  built and logged before it is sent, and Gemini rejects the key at once. The
  same method reproduced the option-off sizes of the real runs to the
  character.
- So the option-on **tokens and times below are estimates**, not
  measurements. Input tokens are scaled by prompt characters; output and
  thinking tokens, and time, are scaled by item count, at each passage's own
  measured option-off rate.

## Passages

Variant notes (`⧫ (…)`) were removed. Paragraphs are separated by a blank line.

| Name | Reference | Paragraphs | Words |
|---|---|---|---|
| DN 23 | `dn23/pli/ms`, the paragraph "Tena kho pana samayena pāyāsissa rājaññassa evarūpaṁ pāpakaṁ diṭṭhigataṁ uppannaṁ hoti …" | 1 | 23 |
| MN 1 | `mn1/pli/ms`, the paragraph "Idha, bhikkhave, assutavā puthujjano … ‘Apariññātaṁ tassā’ti vadāmi." (earth) | 1 | 39 |
| SN 56.11 | `sn56.11/pli/ms`, "Ekaṁ samayaṁ bhagavā bārāṇasiyaṁ …" and "Dveme, bhikkhave, antā …" | 2 | 53 |
| Dhp 1–5 | `dhp1-20/pli/ms`, verses 1–5, one paragraph each | 5 | 75 |
| AN 4.41 | `an4.41/pli/ms`, the whole sutta from "Catasso imā, bhikkhave, samādhibhāvanā" to the verse, without "Paṭhamaṁ." | 7 | 227 |
| Dhp-a (long) | `dhp1-20.att/pli/cst`, the paragraph "Tasmiṁ arahattaṁ patte satthā bhikkhusaṅghaparivuto cārikaṁ caramāno setabyaṁ gantvā siṁsapāvanaṁ pāvisi. …" | 1 | 329 |

## FR-G7: the cost table

"Thinking" is Gemini's thinking tokens. They are counted against
`max_output_tokens` and billed as output. Option off: measured. Option on: items and
characters measured, the rest estimated (see above).

| Passage | Items off → on | Prompt chars off → on | Tokens off (in / out / thinking) | Total tokens off → on (est.) | Time off → on (est.) |
|---|---|---|---|---|---|
| DN 23 | 28 → 32 (+14 %) | 24 397 → 25 484 (+4.5 %) | 8 401 / 1 087 / 5 540 | 15 028 → 16 349 (**+8.8 %**) | 46 s → 52 s |
| MN 1 | 48 → 58 (+21 %) | 36 799 → 39 738 (+8.0 %) | 12 825 / 1 682 / 5 948 | 20 455 → 23 069 (**+12.8 %**) | 44 s → 53 s |
| SN 56.11 | 60 → 85 (+42 %) | 50 064 → 57 472 (+14.8 %) | 16 914 / 2 261 / 12 545 | 31 720 → 40 392 (**+27.3 %**) | 97 s → 137 s |
| Dhp 1–5 | 93 → 100 (+8 %) | 77 750 → 79 624 (+2.4 %) | 25 850 / 3 705 / 16 262 | 45 817 → 47 943 (**+4.6 %**) | 170 s → 183 s |
| AN 4.41 ¹ | 285 → 345 (+21 %) | 218 344 → 236 352 (+8.2 %) | 66 224 / 11 596 / 27 839 | 105 659 → 119 423 (**+13.0 %**) | 344 s → 417 s |
| **All five** | | | | 218 679 → 247 176 (**+13.0 %**) | 701 s → 842 s (+20 %) |
| Dhp-a (long) ² | 378 → 486 (+29 %) | 237 585 → 269 509 (+13.4 %) | — | — | — |

¹ One of the 7 requests failed with a transport error. Its prompt characters
are in the table, but its tokens are not.
² One request even with the option off. Gemini answered "overloaded", so it has
no token numbers.

**Why the increase is small.** With the option off, the request already asks for
the reading and the contextual meaning of every word with several grammar
readings (the one-option items, 5–47 per passage). In the option-off sessions the
Contextual cell was already filled for 72–92 % of the rows with a definition
(DN 23 28/33, MN 1 48/59, SN 56.11 60/85, Dhp 1–5 91/99, AN 4.41 251/347). The
option adds the rest: indeclinables (`kho`, `pana`), one-reading words, and the
components of compounds. Most of the prompt is the options of the ambiguous
words, and the added items have one option each.

**How far the estimate can be off.** The added items are cheap to answer (one
option, nothing to choose). Even if each cost **twice** the average item in
output and thinking, the increase would be +15 % (DN 23), +21 % (MN 1), +47 %
(SN 56.11), +8 % (Dhp 1–5) and +21 % (AN 4.41). That is still under the 50 %
threshold for every passage.

**FR-G8, per-reading lists (option off).** The answers carried a
`contextual_meanings` list for 15 (DN 23), 21 (MN 1), 26 (SN 56.11), 39 (Dhp 1–5)
and 135 (AN 4.41) readings. **None was dropped** for a wrong length or a
non-string entry. Their output-token cost could not be measured: the comparison
run without the prompt sentence was rate-limited.

## FR-G7 step 5: senses changed by the option

**Not measured.** It needs option-on answers. When there is quota, run the same
CLI command with `--contextual-all` and compare `selected_index`,
`selected_deconstruction_index` and `component_selected_uids` per word with the
option-off session JSON. `analyse.py` in the scratchpad did this; it is not kept
in the repo.

## FR-G7 step 4: the decision

By the provisional rule (≤ 50 % more total tokens → default **on**), the estimate
(+13 % overall, +27 % for the worst passage, and under 50 % even at the doubled
bound) points to **default on**. The rule asked for measured tokens, and these
are estimates. The quality check (senses changed) is also missing. **For the
maintainer to confirm.** `gloss_contextual_all` stays `false` until then.

## 7.3a: response size, and a follow-up

The option is not the main problem here. The size of one paragraph already is,
even with the option off:

- The largest answer with the option off, output plus thinking, was **11 622
  tokens**: one AN 4.41 paragraph, with a prompt of 64 987 characters. That is
  **71 %** of `max_output_tokens: 16384`, and above the 60 % mark in the task.
- The long Dhp-a paragraph is **one request of 378 items and 237 585 prompt
  characters** with the option off (486 items, 269 509 characters with it on).
  At the rates measured above (about 39 output tokens and about 150 thinking
  tokens per item), its answer would need well over 16 384 output tokens, so it
  would be cut off, with the option on or off.
- Requests are split only between paragraphs
  (`WORD_SELECTION_BATCH_CHAR_LIMIT` = 40 000 characters). A single paragraph
  is never split, so AN 4.41's paragraphs (up to 65 000 characters) and the
  Dhp-a paragraph go as one request each.

**Proposed follow-up** (added to the task list as 7.7): split a paragraph's
items into chunks in `gloss_word_selection_run.rs`, at the same character
limit, keeping a compound's break-down item and its component items in one
chunk.

## FR-J4: the translation comparison

**Not run.** It was rate-limited like the rest. Per the PRD, the translation word
table keeps **(a) both columns**, Contextual and Definition, as implemented in
6.0. The maintainer can still choose (b). The temporary switch for the
comparison was removed.
