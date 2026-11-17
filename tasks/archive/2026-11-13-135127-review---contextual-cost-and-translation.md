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

## PRD section K: the word-selection request before and after

Model: `ministral-8b-latest` (Mistral free key), 2026-11-14. `simsapa gloss
--no-translate --no-contextual-all --format json`, on DPD's five fixture
passages (the text of DPD's own requests, from the fixtures' `ai_debug.json`).
**Baseline:** the CLI built before section K, with the previous default
request prompt. **New:** the current build, which upgrades that stored prompt
to the new default on load, as it does for a user who never edited it. Both
runs held `cli-write.lock`, so neither wrote cache rows. Agreement is
measured against the senses in DPD's `study.md`, on top-level tokens with
two or more candidate senses, aligned by word. DPD's choices were made by an
AI (Gemini 3.5 Flash), so they are a consistent reference, not the truth.

### What could be run

The free key allows little per minute. 18 requests were sent: 7 were
rate-limited and 2 had answers in the wrong shape (below), so 9 gave answers.
The comparison below uses the three passages where both requests gave
answers. TH215's new request and SN15.1's baseline never did.

| Passage | Senses: baseline → new | Readings: baseline → new | Default ranking, no AI | Input tokens: baseline → new |
|---|---|---|---|---|
| DHP211 | 10/13 → 10/13 | 0/4 → 1/5 | 11/13 | 5 354 → 5 917 |
| AN3.33 ¶1 | 27/36 → 28/36 | 9/10 → 8/11 | 33/36 | 22 306 → 23 100 |
| MN41 ¶2 | 49/55 → 49/55 | 17/27 → 21/27 | 52/55 | 31 639 → 32 541 |
| **Together** | **86/104 → 87/104** | **26/41 → 30/43** | **96/104** | +4 % |

### What it shows

- **No measurable change in sense agreement with this model** (86 → 87 of
  104), and a small gain in readings (63 % → 70 %). The baseline and the new
  request differ in five ways at once, and there is one run of each, so a
  difference of one or two senses is noise.
- **`ministral-8b` agrees with DPD less often than Simsapa's default ranking
  with no AI** (86–87 against 96 of 104). Part of this is how the reference
  was made: the default ranking ports DPD's own ranking (docs/dpd-analysis-procedure.md
  §1.6), so it starts close to DPD's choices. Even so, with this model the AI
  changes more right defaults to wrong ones than the reverse. A small model
  is no gain for word selection.
- **The uid rescue never triggered.** The model copied every lemma exactly.
  Its mistakes were `grammar_index` values out of range (2–3 per large
  request, in both variants). Such an entry is rejected as a whole, so its
  sense answer is lost too.
- **Answers in the wrong shape, new request only.** 2 of the 6 new requests
  that were not rate-limited were rejected with "Response JSON has no
  'selections' array". The baseline had none in 4. The same DHP211 request
  succeeded on the next try. The logged start of one rejected body was a code
  fence. The rest was not recorded, so the cause is not known. One guess is
  that the model wrote another JSON object, such as a paragraph entry, before
  the selections. The parser takes the first JSON object in the reply.
- **Size.** The new request is 4 % more input tokens (the paragraph text and
  the rules), and output is about the same.

### Follow-ups (task list 9.8, 9.9)

- In lenient mode, drop an out-of-range `grammar_index` (with a log line)
  and keep the entry's sense and contextual meaning, instead of rejecting
  the entry.
- Make the reply parser take the first JSON object that has a `selections`
  array, not the first JSON object. The validator now logs the start of a
  rejected body (`ai_engine.rs`), so the next wrong-shape reply will show
  its cause.
- Repeat this comparison with a stronger model before drawing conclusions
  about section K's quality effect.


## Gemini runs (second key, 2026-11-14)

`gemini-3.6-flash` for word selection, `gemini-3-flash-preview` for the
translations (a separate per-model quota). 21 requests. Same passages,
binaries, settings and scoring as the Mistral runs above.

### Section K: before and after

12 of the 15 word-selection requests gave answers. MN41 ¶2's baseline failed
with a transport error. Two replies were cut off (see "Response size").

| | Senses agreeing with DPD (4 passages: TH215, DHP211, AN3.33 ¶1, SN15.1 ¶2) |
|---|---|
| Default ranking, no AI | 101/116 |
| Baseline request | 108/116 |
| New request (section K) | 106/116 |

- With Gemini the AI improves on the default ranking (+5 to +7 senses),
  unlike `ministral-8b`.
- The baseline and the new request differ by noise. Three tokens changed
  each way between the runs: `piyaṁ` (the same token picks a different
  `piya` sense from run to run), `ca`, `tibbaṁ`, `gaccheyya`, and
  `āyasmantaṁ`. The DPD fixture files disagree among themselves on
  `āyasmantaṁ` (`SOURCE.md`).
- Readings: 56/65 against 55/64.
- The uid rescue did not trigger. No `grammar_index` was out of range, and
  no reply had the wrong shape.
- **Cost of section K:** the new request has 3–11 % more input tokens. It
  also makes the model write and think more: AN3.33 output 1 878 → 3 422
  tokens and thinking 4 095 → 7 141; DHP211 output 596 → 779 and thinking
  4 296 → 4 961. The extra output is mostly the `uid` echo.

### FR-G7 step 5 (task 7.3): does "Contextual meaning for every word" change the senses?

On the three passages where both runs gave answers, 2 of 52 ambiguous
senses differ: `piyaṁ`, and `āyasmantaṁ`, which is closer to DPD with the
option on. Both are tokens that also change between two runs without the
option. **No sign that the larger request dilutes the sense choices.**

### FR-G7 (task 7.2): measured cost of the option

| Passage | Total tokens off → on (in + out + thinking) |
|---|---|
| TH215 | 5 947 → 8 107 (+36 %) |
| DHP211 | 11 286 → 13 233 (+17 %) |
| AN3.33 ¶1 | 31 719 → 35 183 (+11 %) |
| **Together** | **48 952 → 56 523 (+15.5 %)** |
| SN15.1 ¶2 | 43 201 → cut off (149 items) |

The measured +15.5 % agrees with the estimate (+13 %). By the decision rule
that means **default on**, but see the next section: with the option on,
more paragraphs reach the output limit.

### Response size: replies cut off at the output limit

**Two of the largest requests were cut off mid-JSON:** MN41 ¶2 with the new
request (106 items) and SN15.1 ¶2 with the option on (149 items). Each used
16 380 of the 16 384 `max_output_tokens`, about 14 500 of them on Gemini's
thinking. The engine retries a cut-off reply as `invalid_response`; with
`--model-name` there is nothing to fall back to, so the paragraph got no
answers. So the limit is hit at about 100–150 items by thinking, not by the
answer. With the option off, the old request stayed under it on these
passages (SN15.1: 14 264 output + thinking).

Two ways out:

1. **Raise `max_output_tokens` for Gemini** (`ai_engine.rs`, now 16 384).
   Gemini 3 Flash allows 65 536. It costs nothing unless used. One line.
2. **Split a paragraph's items into chunks** (task 7.7). That keeps each
   request's thinking smaller, and it works for every provider.

Doing the first now and the second as planned would remove the failures.
The option's default should wait for one of them.

### FR-J4 (task 7.5): the translation word table

`gemini-3-flash-preview`, on the rows of the option-on sessions (17 of 20,
18 of 18, and 82 of 88 rows with a Contextual value). (a) is the table with
both columns, as implemented; (b) is Contextual only, with the Definition
where a row has none.

| Passage | Prompt chars (a) / (b) | Time (a) / (b) |
|---|---|---|
| TH215 | 3 085 / 2 490 | 11.7 s / 9.7 s |
| DHP211 | 2 977 / 2 293 | 17.1 s / 9.8 s |
| AN3.33 ¶1 | 9 970 / 6 213 | 29.4 s / 11.7 s |

**TH215.** paṇṇavīsativassāni, yato pabbajito ahaṃ, accharā-saṅghāta-mattam'pi, cetosantim'anajjhagaṃ.
- (a) both columns
  - Translation: In the twenty-five years since I went forth, I have not obtained peace of mind even for the duration of a finger snap.
  - Literal: For twenty-five years, since I went forth, for the time of a finger snap, peace of mind I did not obtain.
- (b) Contextual only
  - Translation: For twenty-five years since I went forth, I have not attained peace of mind for even the snap of a finger.
  - Literal: For twenty-five years, since I went forth, for the time of a finger snap, peace of mind I did not obtain.

**DHP211.** tasmā piyaṃ na kayirātha, piy'āpāyo hi pāpako, ganthā tesaṃ na vijjanti, yesaṃ n'atthi piy'āppiyaṃ.
- (a) both columns
  - Translation: Therefore one should not hold anything dear, for separation from the beloved is evil. Bonds do not exist for those who have no dear or unloved person.
  - Literal: Therefore dear not one should make, separation from the beloved for [is] evil, bonds of them not exist, for whom there is no dear or unloved person.
- (b) Contextual only
  - Translation: Therefore one should not make anything dear, for separation from the beloved is evil; bonds do not exist for those who have no dear or unloved person.
  - Literal: Therefore dear not one should make, separation from the beloved for [is] evil, bonds of them not exist, for whom there is no dear or unloved person.

**AN3.33_p1.** atha kho āyasmā sāriputto yena bhagavā ten'upasaṅkami, upasaṅkamitvā bhagavantaṃ abhivādetvā ekam'antaṃ nisīdi. ekam'antaṃ nisinnaṃ kho āyasmantaṃ sāriputtaṃ bhagavā etadavoca, saṃkhittena'pi kho ahaṃ, sāriputta, dhammaṃ deseyyaṃ, vitthārena'pi kho ahaṃ, sāriputta, dhammaṃ deseyyaṃ, saṃkhittavitthārena'pi kho ahaṃ, sāriputta, dhammaṃ deseyyaṃ, aññātāro ca dullabhā'ti. etassa, bhagavā, kālo, etassa, sugata, kālo yaṃ bhagavā saṃkhittena'pi dhammaṃ deseyya, vitthārena'pi dhammaṃ deseyya, saṃkhittavitthārena'pi dhammaṃ deseyya. bhavissan'ti dhammassa aññātāro'ti.
- (a) both columns
  - Translation: Then Venerable Sāriputta approached where the Blessed One was. Having approached and bowed down to the Blessed One, he sat to one side. When Venerable Sāriputta was seated to one side, the Blessed One said this: 'Sāriputta, I would teach the teaching in brief, I would teach the teaching in detail, and I would teach the teaching in brief and in detail, but those who understand are difficult to find.' 'Blessed One, it is the right time for this; O Accomplished One, it is the right time for this, that the Blessed One would teach the teaching in brief, would teach the teaching in detail, and would teach the teaching in brief and in detail. There will be those who understand the teaching.'
  - Literal: Then indeed Venerable Sāriputta where the Blessed One [was] approached there, having approached, having bowed down to the Blessed One, to one side sat down. To one side seated indeed Venerable Sāriputta the Blessed One said this: 'In brief indeed I, Sāriputta, teaching would teach, in detail indeed I, Sāriputta, teaching would teach, in brief and in detail indeed I, Sāriputta, teaching would teach, those who understand but [are] difficult to find,' thus. 'For this, Blessed One, the right time, for this, O Accomplished One, the right time that the Blessed One in brief teaching would teach, in detail teaching would teach, in brief and in detail teaching would teach. There will be,' thus, 'of the teaching those who understand,' thus.
- (b) Contextual only
  - Translation: Then Venerable Sāriputta went to the Blessed One. After bowing to the Blessed One, he sat down to one side. When he was seated, the Blessed One said to him: 'Sāriputta, I would teach the teaching in brief, in detail, or both in brief and in detail, but those who understand are hard to find.' 'Blessed One, it is the right time for this; Accomplished One, it is the right time for this: that the Blessed One would teach the teaching in brief, in detail, and both in brief and in detail. There will be those who understand the teaching.'
  - Literal: Then indeed Venerable Sāriputta where the Blessed One [was] there approached; having approached, having bowed down to the Blessed One, to one side he sat down. To the indeed to one side seated Venerable Sāriputta, the Blessed One said this: 'In brief indeed I, Sāriputta, the teaching would teach; in detail indeed I, Sāriputta, the teaching would teach; in brief and in detail indeed I, Sāriputta, the teaching would teach; but those who understand [are] hard to find.' 'For this, Blessed One, [is] the right time; for this, O Accomplished One, [is] the right time, that the Blessed One in brief the teaching would teach, in detail the teaching would teach, in brief and in detail the teaching would teach. There will be of the teaching those who understand.'

**Reading.** The two give nearly the same translations, and the literal ones
are identical for TH215 and DHP211. The differences are in the natural
translations and are matters of wording. For DHP211, (a) has "one should not
**hold** anything dear" and (b) "should not make anything dear". The table
gave "one should make" (Contextual) and "should do; could make"
(Definition), so "hold" is the model's own idiom, not a sense taken from the
Definition. For AN3.33, (b) is terser ("went to the Blessed One"). No
translation has a sense error that the other avoids. (b) is shorter and
faster. **(a) stays, as the PRD says, unless the maintainer prefers (b).**
On this evidence the choice is about cost, not quality.

## Decisions taken from these runs

- **The uid echo is taken out again (FR-K6).** Section K first sent every
  option's `uid` and asked for it in every answer, so that a correct uid
  could rescue a misspelled lemma. In about 550 answers from `ministral-8b`
  and `gemini-3.6-flash`, no lemma was misspelled, so the rescue never ran,
  while the echo made the answers much longer (AN3.33 ¶1: 1 878 → 3 422 output
  tokens with Gemini). The prompt now sends a `uid` only where an item's
  options share a `word`, and asks for it only then. The parser still
  accepts a `uid`, and still matches a lemma that differs only in case, `ṃ`
  or spacing.
- **"Contextual meaning for every word" is on by default (FR-G0):** +15.5 %
  tokens, and the senses chosen did not change.
- **The output limit is sized to the request (FR-K8):** Gemini's
  `max_output_tokens` is 8 192 + 300 per item, from 16 384 to 65 536. The
  limit is a cap. The runs show replies as long as they need to be, far
  under the cap on small requests (TH215: about 2 900 of 16 384), so a higher
  cap costs nothing unless a reply needs it.
- **The translation table has one Meaning column (FR-J1, choice (b)):** the
  Contextual value, else the Definition.
