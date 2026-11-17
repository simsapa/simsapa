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

### Sutta check (FR 12)

Each of the 36 `texts-sc-numbering/` files was compared with the Pāli `ms`
text of its number, verse by verse (his first line of each verse beside the
Pāli and Sujato's), and with the book's contents page, which gives every
sutta's Sutta Nipāta verse range. 35 files are the sutta their name says:
the heading names the Pāli sutta (`The Cave` ↔ *Guhaṭṭhakasutta*,
`Quickly` ↔ *Tuvaṭakasutta*, `Ajita's Question` ↔ *Ajitamāṇavapucchā*, …),
the verse count equals the book's range (`Sensual Pleasures [766–771]` = 6
verses), and each of his verses translates the Pāli verse in the same
position. `snp5.18.md` is not (first entry below). SC `snp5.1` (the
introductory verses of the Pārāyanavagga) and SC `snp5.18` (the prose and
verse epilogue, *Pārāyanatthutigāthā*) are not in the book.

### The four suttas with a different verse count

- **File / segments**: `snp1.12.json`, `snp1.12:16.*`–`17.*`.
  - **Source**: 15 verses (book: `Sn 1:12, 207–221`).
  - **Pāli**: 17 verse groups. Groups 16–17 are the *Uragavagga*'s uddāna
    (*Tassuddānaṁ / Urago dhaniyo ceva, …*), the summary of the vagga's
    sutta names; groups 1–15 are the sutta.
  - **Done**: verses 1–15 on groups 1–15; the uddāna and the colophon
    (`15.5`–`15.6`) are `""`.
- **File / segments**: `snp3.6.json`, the prose segments of groups 1–7, 11,
  17, 23, 29, 35, 41 and 52–54.
  - **Source**: 38 verses (book: `Sn 3:6, 510–547`, 38 verses), no prose.
  - **Pāli**: 54 groups: the 38 verses and 16 prose groups (the narrative
    frame: the deity's questions, Sabhiya's visits to the six teachers, his
    reflections, the going forth).
  - **Done**: verse *n* on the *n*-th verse group (groups 8–51); every prose
    segment and every speaker tag (`(iti sabhiyo)`, …) is `""` (FR 9).
- **File / segments**: `snp4.16.json`, `snp4.16:22.*`–`23.*`.
  - **Source**: 21 verses (book: `[955–975]`).
  - **Pāli**: 23 groups; 22–23 are the *Aṭṭhakavagga*'s uddāna.
  - **Done**: verses 1–21 on groups 1–21; the uddāna is `""`.
- **File / segments**: `snp5.18.md` → `snp5.19.json` (all segments).
  - **Source**: `Piṅgiya's Song of Praise`, 19 verses beginning *I shall sing
    in praise of the way to the beyond*; book: `17. Piṅgiya’s Song of Praise
    [1131-49]`.
  - **Pāli**: SC `snp5.18` is *Pārāyanatthutigāthā* (a prose frame and seven
    verses naming the sixteen brahmins; PTS verses 1124–1130). His text is SC
    `snp5.19`, *Pārāyanānugītigāthā* (*Pārāyanamanugāyissaṁ …*), whose 19
    verses match his one for one. The verse numbers settle it: the book's
    range `[1131-49]` is exactly SC `snp5.19`'s PTS verse numbers
    (`vnp1131`–`vnp1149` in `sc_bilara_data/reference/`), and Bodhi's and
    Ṭhānissaro's suttas-core texts give the same `snp5.18` / `snp5.19` split. Groups 20–40 of `snp5.19` are the uddāna of the whole
    Sutta Nipāta and its closing verses.
  - **Done**: the segment file is `bilara/snp5.19.json` (`input`:
    `texts-sc-numbering/snp5.18.md`); verses 1–19 on groups 1–19, groups
    20–40 `""`. The record's uid becomes `snp5.19/en/nyanadipa` (it was
    `snp5.18/en/nyanadipa`, beside the wrong Pāli text). Nothing in the code
    or the navigation data names the old uid. The source file name is a
    numbering error in `texts-sc-numbering/`, to be reported.

### General findings

- **One line per segment.** In 32 of the 36 suttas every verse has exactly as
  many lines as the Pāli verse has `verse-line` segments, and his lines follow
  the Pāli lines in order, so each line is placed on its Pāli line (FR 5). The
  four others (`snp1.12`, `snp4.15`, `snp5.4`, `snp5.8`) are the entries
  below; every other verse of theirs is placed the same way. No segment holds
  more than one of his lines.
- **Speaker tags and colophons.** The Pāli `speaker` segments
  (`(iccāyasmā ajito)`, `(ajitāti bhagavā)`) and the `endsutta` /
  `endsuttapart` colophons (*Kāmasuttaṁ paṭhamaṁ*) have no translation; they
  are `""`.
- **Apostrophes.** Sixteen of his Markdown headings (`Ajita's Question`, …,
  all in the Pārāyanavagga) have a straight `'`; everything else in his text,
  and the book, has `’`. The title segments have `’`, as the HTML conversion
  of the Markdown set it before (FR 4), so the record titles stay as they
  were. The check command applies the same rule (`curly_apostrophes()`)
  to the source before comparing.
- **Italics.** He sets a questioner's verses in italics (Pārāyanavagga,
  Sāriputta's questions in `snp4.16`, Sabhiya's in `snp3.6`). Each italic
  line becomes `<i>…</i>` in its segment.
- **Footnotes.** All 29 notes (`[^1]`–`[^29]`, the book's *Notes* 1–29, none
  missing) are in `footnotes.json`, each on the word its marker follows. In
  `snp4.13:14.4` the marker is inside a bracket, `[themselves[^14]]`; the
  note follows the word `[themselves]`, and the renderer puts the marker
  before the closing bracket, as in the source. The literal `<` of note 16
  (`kappaṃ < kappyaṃ`) is escaped as `&lt;`.

### Placement entries

- **File / segments**: `snp4.15.json`, `snp4.15:6.1`.
  - **Source**: an unnumbered italic line between verses 5 and 6, “*At this
    point the training rules are recited:*”; his verse 6 has four lines.
  - **Pāli**: verse 6 has five lines; the first, *Tattha sikkhānugīyanti*,
    is that sentence.
  - **Done**: the italic line in `6.1`, his four lines of verse 6 in
    `6.2`–`6.5`.
- **File / segments**: `snp5.19.json`, `snp5.19:1.1`–`1.2`.
  - **Source**: his first line ends with the speaker attribution in
    brackets: “I shall sing in praise of the way to the beyond, [this the
    Venerable Piṅgiya said.]”
  - **Pāli**: the attribution is a separate speaker segment,
    `1.2` *(iccāyasmā piṅgiyo)*.
  - **Done**: the whole line in `1.1`, as he wrote it; `1.2` is `""`.
- **File / segments**: `snp5.4.json`, `snp5.4:2.1`–`2.6`, `3.1`–`3.7`.
  - **Source**: verses 2 and 3 open with the same two lines, “All these human
    sages, warriors and brāhmaṇas / who make sacrifices to the gods in
    manyfold ways here in the world”; verse 3 has four lines in all.
  - **Pāli**: three lines, *Ye kecime isayo manujā / Khattiyā brāhmaṇā
    devatānaṁ / Yaññamakappayiṁsu puthūdha loke*; verse 3 ends with
    *Pucchāmi taṁ bhagavā brūhi me taṁ* (“I ask you, Blessed One; please tell
    me this”).
  - **Done**: his first line (sages, men, warriors, brāhmaṇas) in `2.1` /
    `3.1`, his second (sacrifices to the gods in the world) in `2.4` /
    `3.4`; `2.3` and `3.3` are `""`. Verse 3's last two lines in `3.5`–`3.6`;
    `3.7`, which he does not translate, is `""`.
- **File / segments**: `snp5.8.json`, `snp5.8:3.1`–`3.8`.
  - **Source**: six lines, ending “have crossed birth and old age, Venerable
    Sir.”
  - **Pāli**: seven verse lines; the last, *Pucchāmi taṁ bhagavā brūhi me
    taṁ*, has no counterpart in his verse.
  - **Done**: his lines in `3.1`, `3.3`–`3.7`; `3.8` is `""`.
- **File / segments**: `snp5.8.json`, `snp5.8:7.1`–`7.7`.
  - **Source**: five lines: “In that I do delight, in the word of the great
    sage. / Those who have abandoned the seen, the heard, / thought, as well
    as virtue and practice, / and who have fully known craving and are taint
    free, / I too say, “They have crossed the flood.””
  - **Pāli**: seven lines; `7.2` *Sukittitaṁ gotamanūpadhīkaṁ* (well
    proclaimed, Gotama, free of attachment) and `7.5` *Anekarūpampi pahāya
    sabbaṁ* (having abandoned the manifold too) have no counterpart.
  - **Done**: his lines in `7.1`, `7.3`, `7.4`, `7.6`, `7.7`; `7.2` and `7.5`
    are `""`.

### Corrections

- **File / segments**: `snp1.12.json`, `snp1.12:14.2`.
  - **Source**: verse 14's second line is broken in two, “the householder who
    feeds a family and the possessionless one of good / practice.”, the
    Markdown following the book's page layout, which wraps this over-long
    line.
  - **Pāli**: one line, *Gihī dāraposī amamo ca subbato*.
  - **Done**: one line in `14.2`, “… the possessionless one of good
    practice.”, so the reader does not show “practice.” as a verse line of
    its own.
- **File / segments**: `snp4.7.json`, `snp4.7:6.2`–`6.3`.
  - **Source**: `[in defence] when blamed by others.` has no Markdown line
    break before `And that is his big blunder:`, so the Markdown joins the
    two lines; the book prints them as two lines.
  - **Pāli**: two lines, `6.2` and `6.3`.
  - **Done**: one line in each, as in the book.

### Source defects (kept as written, to report)

- `texts-sc-numbering/snp5.18.md` holds SC `snp5.19` (entry above).
- `texts-sc-numbering/snp5.7.md`: the heading is `Upasīva'S Question`, with a
  capital `S`; the book has `UPASĪVA’S QUESTION` and, on the contents page,
  `Upasīva’s Question`. The title segment `snp5.7:0.2` keeps the heading as
  written (FR 12), `Upasīva’S Question`, with only its apostrophe curly.
- `snp5.18.md` verse 2 (and the book): “The men who has abandoned the stain of
  delusion” (*Pahīnamalamohassa*, singular), presumably for “The man who has
  abandoned”. Kept.
- `texts-sc-numbering/snp4.7.md` line 29: a missing hard line break (entry
  above).

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
