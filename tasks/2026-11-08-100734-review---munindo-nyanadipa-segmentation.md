# Review: Munindo and Nyanadipa segmentation

PRD: [2026-11-08-100734-prd---segmented-munindo-nyanadipa-and-dhp-verse-numbers.md](./2026-11-08-100734-prd---segmented-munindo-nyanadipa-and-dhp-verse-numbers.md)
(FR 6, 7, 12, 13, 22).

Every place where a source file and the Pāli do not match, and every placement
decision that is not a plain one-to-one reading, has one entry here. The
source `markdown/` and `texts-sc-numbering/` files are never edited; the fix,
where one was needed, is in the segment file.

Entry format:

- **File / segments**: the segment file and the segment ids concerned.
- **Source**: what the translator's text has.
- **Pāli**: what the Pāli has.
- **Done**: what the segment file does.

## Munindo, *A Dhammapada for Contemplation*

### Chapter check (FR 12)

All 26 chapter files `markdown/dhp-N.md` are the translation of
`DHP_CHAPTERS_TO_RANGE[N]`: each chapter title answers to the Pāli vagga
(e.g. `Great Being` ↔ *Brāhmaṇavagga*, `Various` ↔ *Pakiṇṇakavagga*), the
first and last verse numbers are the first and last verses of the range, and
Munindo's verse numbers cover every verse of the range with no gap and no verse
outside it. No chapter is missing a verse by number.

### General findings

- **Munindo's verse numbers.** Every line was placed by its meaning (FR 7).
  In no case does a line belong to a Pāli verse outside the number Munindo
  puts on its verse: his single numbers and his ranges (`58-59.`,
  `137-140.`, …) always cover the verses his lines translate. No entry of the
  kind “his number disagrees with the placement” was needed.
- **Missing verses.** Every one of the 423 Pāli verses has at least one of
  his lines. He often leaves out a Pāli line or image (e.g. the warrior and
  the meditator of Dhp 387, the mustard seed of Dhp 401, the king's
  punishment of Dhp 310); those segments are `""`. These omissions are his
  free rendering, not defects, and are not listed one by one.
- **Line order within a verse.** Munindo often reverses the parts of a verse
  (simile before application, conclusion first). His order is kept (FR 5): the
  lines go to the segments whose meaning is closest while staying in his
  order, so some of his lines stand beside a Pāli line they do not translate
  (e.g. Dhp 1, 47, 64, 210, 337). These are not listed one by one; the
  entries below are the cases where a verse boundary falls inside his
  sentence, or where the result needs a note.
- **Dhp 416.** The Pāli root (`dhp383-423_root-pli-ms.json`) repeats Dhp 416
  under a second verse story, *Jotikattheravatthu* (`dhp416:5.0`, then
  `dhp416:5.1`, `6`–`8`). Munindo has the verse once; it is placed in
  `dhp416:1`–`4`, and the repeat is `""`. The repeat's heading `dhp416:5.0`
  is not a verse-group heading for the range function (FR 18), so it gets no
  range.

### Placement entries

- **File / segments**: `dhp44-59.json`, `dhp58:1`–`dhp59:4`.
  - **Source**: Munindo's `58-59.` is one sentence of four lines: the lotus growing from discarded waste, and the disciple's radiance outshining ignorance.
  - **Pāli**: Dhp 58 is the simile (lotus on a rubbish heap), Dhp 59 its application (the disciple among the blind worldlings).
  - **Done**: lines 1–2 (the simile) in `dhp58:1`–`2`, lines 3–4 in `dhp59:3`–`4`. The split falls inside his sentence, at the comma between simile and application.
- **File / segments**: `dhp76-89.json`, `dhp87:1`–`dhp88:3`.
  - **Source**: Munindo's `87-88.` is nine lines; the sentence “To pursue such release is difficult and rare, / yet the wise will seek it, …” runs from the seclusion of Dhp 87 into the delight of Dhp 88.
  - **Pāli**: Dhp 87 ends *Viveke yattha dūramaṁ* (seclusion, where it is hard to delight); Dhp 88 begins *Tatrābhiratimiccheyya* (one should seek delight there).
  - **Done**: lines 1–6 in Dhp 87 (the last, “difficult and rare”, beside *dūramaṁ*), lines 7–9 in Dhp 88. The split falls inside his sentence, at the comma.
- **File / segments**: `dhp129-145.json`, `dhp137:1`–`dhp140:4`.
  - **Source**: Munindo's `137-140.` is one eight-line sentence: harming the defenceless brings suffering; “pain or poverty or loss, / illness, insanity or persecution, / abuse, distress or devastation”; and after death, hell. His list is shorter than the Pāli's ten and in a different order.
  - **Pāli**: Dhp 137 (harming the innocent, one of ten states), Dhp 138–139 (the ten states), Dhp 140 (fire, and hell after death).
  - **Done**: lines 1–3 in Dhp 137, “pain or poverty or loss” beside `dhp138:1`, “illness, insanity or persecution” beside `dhp138:3`, “abuse, distress or devastation” beside `dhp139:2` (*abbhakkhānaṁ*), the last two lines in `dhp140:3`–`4`. The verse boundaries fall inside his sentence.
- **File / segments**: `dhp179-196.json`, `dhp186:1`–`dhp187:4`.
  - **Source**: Munindo's `186-187.` is seven lines in two sentences; the first, “Not in great wealth / is there contentment, / nor in sensual pleasure, / gross or refined.”, ends with the refined (heavenly) pleasures of Dhp 187.
  - **Pāli**: Dhp 186 (no satisfaction in sensual pleasures even from a rain of coins), Dhp 187 (no delight even in heavenly pleasures; delight in the ending of craving).
  - **Done**: lines 1–3 in Dhp 186, “gross or refined.” beside `dhp187:1` (*Api dibbesu kāmesu*), the second sentence in `dhp187:3`–`4`. The split falls inside his first sentence. Dhp 186:3–4 (*Appassādā dukhā kāmā, iti viññāya paṇḍito*) has no counterpart in his verse and is empty.
- **File / segments**: `dhp179-196.json`, `dhp190:1`–`dhp191:3`.
  - **Source**: Munindo's `190-191.` is one sentence: refuge in the three jewels, “sees with penetrating insight: / suffering, its cause, its release / and the Way leading to true freedom.”
  - **Pāli**: the same sentence runs across Dhp 190 (refuge; sees the four noble truths) and Dhp 191 (the four truths named).
  - **Done**: lines 1–3 in Dhp 190, lines 4–5 in `dhp191:1` and `dhp191:3`. The split falls at his colon, as in the Pāli.
- **File / segments**: `dhp179-196.json`, `dhp195:1`–`dhp196:1`.
  - **Source**: Munindo's `195-196.` begins with the conclusion of Dhp 196 (“Immeasurable is the benefit / obtained from honouring those / who are pure and beyond fear.”) and ends with “are worthy of honour” (*pūjārahe*, Dhp 195:1).
  - **Pāli**: Dhp 195 (honouring those worthy of honour, who have crossed sorrow), Dhp 196 (honouring such ones, quenched and fearless, the merit cannot be measured).
  - **Done**: his order is kept (FR 5): lines 1–5 in Dhp 195 (“from sorrow and grieving” beside `dhp195:4` *tiṇṇasokapariddave*), “are worthy of honour.” beside `dhp196:1` (*Te tādise pūjayato*). His first line, the immeasurable merit, stands beside Dhp 195:1, not beside `dhp196:3` where the Pāli says it.
- **File / segments**: `dhp209-220.json`, `dhp219:3`–`dhp220:2`.
  - **Source**: Munindo's `219-220.` is one sentence: “Just as family and friends / joyfully welcome home / loved ones returned from afar, / so their own good deeds / welcome those who have done them / as they go from this life to the next.”
  - **Pāli**: Dhp 219 (relatives welcome one long away), Dhp 220 (so merits receive the one who did good, gone to the next world).
  - **Done**: lines 1–3 in `dhp219:3`–`4`, lines 4–6 in `dhp220:1`–`2`. The split falls inside his sentence, at the comma between simile and application.
- **File / segments**: `dhp235-255.json`, `dhp246:1`–`dhp247:4`.
  - **Source**: Munindo's `246-247.` is one sentence listing the five misdeeds and their result (“destroys the very roots / of their own life”). He has “is sexually irresponsible” before “takes what is not rightfully theirs”.
  - **Pāli**: Dhp 246 (killing, lying, taking what is not given, going to others' wives), Dhp 247 (intoxicants; digs up his own root).
  - **Done**: one line per segment in Dhp 246 in his order (so “is sexually irresponsible” stands beside `dhp246:3` and “takes what is not rightfully theirs” beside `dhp246:4`), the intoxicants line in `dhp247:1`, the last two lines in `dhp247:4`. The verse boundary falls inside his sentence.
- **File / segments**: `dhp235-255.json`, `dhp249:5`–`dhp250:4`.
  - **Source**: Munindo's `249-250.` ends with one sentence across the two verses: “our meditation will be filled / with endless mental affliction; / but if we are free from this discontent, / our meditation is full of peace.”
  - **Pāli**: Dhp 249 ends with the discontented one not attaining concentration; Dhp 250 is the one in whom that is cut off.
  - **Done**: lines 1–6 in Dhp 249, “but if we are free from this discontent,” in `dhp250:1`, the last line in `dhp250:4`. The split falls at his semicolon.
- **File / segments**: `dhp256-272.json`, `dhp268:3`–`dhp269:4`.
  - **Source**: Munindo's `268-269.`: “Like one holding scales / a sage weighs things up, / wholesome and unwholesome, / and comes to know / both the inner and outer worlds.”
  - **Pāli**: Dhp 268 ends with the sage taking up the scales and choosing the best; Dhp 269 begins with avoiding evils.
  - **Done**: lines 1–4 in Dhp 268, “wholesome and unwholesome,” beside `dhp269:1` (*Pāpāni parivajjeti*), the rest in `dhp269:3`–`4`. The verse boundary falls inside his sentence.
- **File / segments**: `dhp256-272.json`, `dhp271:1`–`dhp272:4`.
  - **Source**: Munindo's `271-272.` is three sentences; the second, “Do not feel satisfied because you / attain meditative absorption, / nor because you can dwell in / the bliss of solitude.”, runs from Dhp 271:3 (*samādhilābhena*) to the bliss of renunciation of Dhp 272:1. His opening “Do not rest contented” answers to `dhp272:3` (*Bhikkhu vissāsamāpādi*), which the Pāli says last.
  - **Pāli**: Dhp 271 (not by virtue, learning, concentration or secluded dwelling), Dhp 272 (nor by touching the bliss of renunciation; a bhikkhu should not rest content until the taints are ended).
  - **Done**: his order kept: lines 1–5 in Dhp 271, “nor because you can dwell in / the bliss of solitude.” beside `dhp272:1` (*Phusāmi nekkhammasukhaṁ*), the third sentence in `dhp272:4`. The verse boundary falls inside his second sentence.
- **File / segments**: `dhp360-382.json`, `dhp375:1`–`dhp376:6`.
  - **Source**: Munindo's `375-376.` is twelve lines; the list after “This, then, is the beginning / for a renunciate who takes up the training:” runs on from Dhp 375 into Dhp 376 (“seek contentment; / cultivate the company of those …”). He has “seek contentment” after “commit yourself to the instruction”, where the Pāli has *santuṭṭhi* before *pātimokkhe ca saṁvaro*.
  - **Pāli**: Dhp 375 (the beginning: guarding the faculties, contentment, restraint in the Pāṭimokkha), Dhp 376 (good friends, friendliness, skill in conduct, joy, the end of suffering).
  - **Done**: his order kept: lines 1–5 in Dhp 375 (“commit yourself to the instruction, / seek contentment;” together beside `dhp375:4`), lines 6–12 in Dhp 376. The verse boundary falls inside his sentence, at a semicolon of the list.

## Nyanadipa, *The Silent Sages of Old*

## Upstream defects (suttas-core)

- **File / segments**: suttas-core `segmentations/than/dhp235-255.json`,
  `dhp239:0`.
- **Source**: `235-238*`, the endnote citation of Ṭhānissaro's web page for
  the *previous* verse group (Dhp 235–238), carried into the next verse-group
  heading segment by the suttas-core segmentation.
- **Pāli**: `dhp239:0` is the heading *Aññatarabrāhmaṇavatthu*; the
  translation's heading segment should be empty.
- **Done**: `SuttasCoreImporter` overwrites it with the range `239.` and logs a
  warning (FR 22). To be reported to suttas-core.
