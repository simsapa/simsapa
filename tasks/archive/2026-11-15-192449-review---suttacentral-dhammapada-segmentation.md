# Review: segmentation of the SuttaCentral HTML Dhammapada translations

Ānandajoti, Buddharakkhita and Feldmeier, 26 chapters each, from
`sc-data/html_text/en/pli/sutta/kn/dhp/<translator>/dhp/*.html` into
`bootstrap-assets-resources/dhammapada-suttacentral/bilara/<translator>/`.
Method and rules: [docs/munindo-nyanadipa-segmentation.md](../docs/munindo-nyanadipa-segmentation.md) §8.

Every source defect is listed here and kept verbatim in the segment files; the
SuttaCentral pages are never edited. The hand placements are in
`dhammapada-suttacentral/tools/placements.json` (66 verse groups).

`check-segment-files dhammapada-suttacentral`: 78 files, 0 problems (ids,
joined text word for word against each page, hashes, headings, titles,
coverage).

## Source defects, to report to SuttaCentral

- **Ānandajoti, chapter 23 (`dhp320-333.html`)**: every verse anchor has
  `data-uid='dhp1'`; the `id` (`sc320` …) is right. The script reads the `id`.
- **Feldmeier, line wraps**: the pages keep the print edition's line breaks
  inside a verse line, as `<br>` (168 places), e.g. Dhp 1
  `follows the ox’s<br>foot.`, Dhp 17 `following death he is<br>tormented;`,
  Dhp 20 `well-emancipated<br>mind,`. Kept as `\n`; the wrapped part stays in
  its line's segment (a lowercase line continues the line before). The rule
  also joins three lines that may be genuine lines beginning in lowercase
  (Dhp 102 `is one Dhamma word,`, Dhp 218 `is called going upstream.`,
  Dhp 262 `does one who is envious…` / `become exemplary.`): those stay in the
  segment of the line before them. A wrap before a capital is not detected
  (Dhp 289 `the path leading to<br>Nibbāna.`, Dhp 190 `the<br>Dhamma, and the
  Sangha,`); those are placed as two lines.
- **Feldmeier, lines anchored to the wrong verse**:
  - Dhp 229–230: `Whose conduct is faultless,` and `Who is intelligent,
    endowed with wisdom and virtue,` stand after the `SC 230` anchor but
    translate Dhp 229's pādas 3–4 (*acchiddavuttiṁ medhāviṁ,
    paññāsīlasamāhitaṁ*). Placed in `dhp229:3`, `dhp229:4`.
  - Dhp 375–376: `Association with virtuous friends / Whose way of life is
    pure and unwearied.` stand before the `SC 376` anchor but translate
    Dhp 376's pādas 1–2 (*mitte bhajassu kalyāṇe, suddhājīve atandite*).
    Placed in `dhp376:1`, `dhp376:2`.
- **Feldmeier, typos** (kept): Dhp 217 `does the work that that is his`,
  Dhp 267 `This one is truly is called monk.`
- **Ānandajoti, Dhp 383**: `brahmin.` alone on a line after `be one who knows
  that which is not made,`. Kept, in `dhp383:4`.

## Translations that join verses

Not defects: the translator's own rendering.

- **Buddharakkhita** translates Dhp 87–88, 104–105, 138–140, 186–187,
  190–191, 195–196, 246–247, 271–272 and 345–346 as one paragraph each
  (adjacent anchors with no text between). Each is one verse group: its
  sentences go to the verses they translate where a sentence boundary allows
  (87–88, 104–105, 138–140, 186–187, 345–346); a single sentence covering all
  its verses starts at the first verse's first segment (190–191, 195–196,
  246–247, 271–272).
- **Feldmeier** often runs a sentence across his verse boundaries in the same
  way as SuttaCentral's anchors; only the two cases above needed a move across
  an anchor.

## Placement decisions

- **Ānandajoti**: each of his lines is a Pāli half-verse, so a verse with half
  as many lines as pādas gets one line per half-verse (`dhpN:1`, `dhpN:3`);
  the others were aligned and read. One correction: Dhp 380, his second line
  begins with pāda 4 (*tasmā saṁyamamattānaṁ*) → `dhp380:4`.
- **Buddharakkhita**: prose, split into sentences and clauses. Each verse group
  with more than one unit was read; 23 corrected, mainly where a sentence
  covering two or three pādas was pulled to the pāda after its first (e.g.
  Dhp 5 `By non-hatred alone is hatred appeased.` → `dhp5:3`; Dhp 76, 104–105,
  208, 278, 315, 380).
- **Feldmeier**: every verse whose line count differs from the Pāli was read;
  42 corrected. Typical cases: the second and third lines of Dhp 1–2 both
  translate *manoseṭṭhā manomayā* (`dhp1:2`); a line covering pādas 3–4 placed
  at 4 (Dhp 85, 152, 169, 170, 172, 205); his reversed similes (Dhp 287,
  Dhp 376's last two lines).
- Where the translator's order differs from the Pāli, the lines stay in his
  order (§2), so some stand beside a pāda they do not translate (e.g.
  Feldmeier Dhp 38, 42, 90, 310; Buddharakkhita Dhp 288).
- **Verse groups with as many units as Pāli lines** are placed one per
  segment by rule and were not read one by one (Ānandajoti 69,
  Buddharakkhita 18, Feldmeier 212).

## Effects on the records

- **Titles.** The record titles were SuttaCentral's chapter names (Sujato's:
  `1. Pairs `, with number and trailing space). They are now each
  translator's own `<h1>` without the number: Ānandajoti
  `The Chapter about the Pairs`, Buddharakkhita `The Holy Man` (Dhp 383–423),
  Feldmeier `Thrashing` (Dhp 129–145). The title segment reads
  `<chapter>. <title>`.
- **Footer.** The HTML records showed each page's footer (translator, edition,
  licence, "Prepared for SuttaCentral by"). The segmented records do not:
  the holder and licence are in `suttas.copyright` / `suttas.license`, which
  the reader does not display (the same as the suttas-core records).
  Buddharakkhita's BPS terms ask that the licence text go with every copy;
  his `license` field holds it in full. Showing these fields in the reader is
  a separate change.
- `content_plain` of these 78 records changes (re-bootstrap and index
  rebuild); the uids do not, so the navigation tree is unaffected.
