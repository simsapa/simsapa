# suttas-core: questions for the upstream author

Points found while planning Simsapa's import of segmented translations from
[suttas-core](https://git.unxr.net/adicco/suttas-core) (PRD:
`tasks/2026-11-02-132118-prd---suttas-core-segmented-translations-import.md`).
They are differences from SuttaCentral's Bilara data that Simsapa can work
around, but where we would like to know the intent before relying on them.

Measured against:

- suttas-core `e863bb74` (2026-10-31), database
  `bootstrap-assets-resources/suttas-core-db/2026-10-31/suttas.sqlite`;
- SuttaCentral `sc-data` `2d9ca8da8` (2026-10-17), as imported into Simsapa.

The queries below run against the suttas-core database with `sqlite3`.

## 1. Brahmali's Pātimokkhas are split into one sutta per rule

SuttaCentral has each Pātimokkha as **one** text, with one Bilara file per
layer:

| SuttaCentral uid | `translation/en/brahmali` keys |
|---|---|
| `pli-tv-bu-pm` (monks') | 754 |
| `pli-tv-bi-pm` (nuns') | 946 |

suttas-core has **no** `pli-tv-bu-pm` or `pli-tv-bi-pm` sutta. Its
`data/suttas.json` lists the rules and sections instead, 546 ids in all
(231 for the monks', 315 for the nuns'), for both Brahmali and the Pāli `ms`
root:

``` sql
SELECT sutta_id, COUNT(*) AS segments
FROM sutta_segments
WHERE translator = 'brahmali' AND sutta_id LIKE 'pli-tv-bu-pm-%'
GROUP BY sutta_id ORDER BY MIN(ord) LIMIT 6;
```

The segment ids are SuttaCentral's, unchanged; only the sutta they are filed
under is new. Examples:

| suttas-core `sutta_id` | first segment | last segment | segments |
|---|---|---|---|
| `pli-tv-bi-pm-pubbakarana` | `pli-tv-bi-pm:0.2` | `pli-tv-bi-pm:1.2` | 4 |
| `pli-tv-bu-pm-nidana` | `pli-tv-bu-pm:5.0` | `pli-tv-bu-pm:7.6` | 22 |
| `pli-tv-bu-pm-pj1` | `pli-tv-bu-pm:8.0` | `pli-tv-bu-pm:9.1` | 4 |
| `pli-tv-bu-pm-pj2` | `pli-tv-bu-pm:10.0` | `pli-tv-bu-pm:10.3` | 4 |
| `pli-tv-bu-pm-ay1` | `pli-tv-bu-pm:29.0` | `pli-tv-bu-pm:30.5` | 8 |
| `pli-tv-bu-pm-pc1` | `pli-tv-bu-pm:65.0` | `pli-tv-bu-pm:66.1` | 5 |
| `pli-tv-bi-pm-pj5` | `pli-tv-bi-pm:13.0` | `pli-tv-bi-pm:13.1` | 2 |

`pli-tv-bu-pm-pj1`, as stored:

| segment | content |
|---|---|
| `pli-tv-bu-pm:8.0` | The recitation of the rules on expulsion |
| `pli-tv-bu-pm:8.1` | These four rules on expulsion come up for recitation. |
| `pli-tv-bu-pm:9.0` | Expulsion 1: sexual intercourse |
| `pli-tv-bu-pm:9.1` | If a monk, after taking on the monks’ training and way of life, … |

Per-rule ids by prefix (both Pātimokkhas; `bu` = monks', `bi` = nuns'):
`-pj` 4 / 8, `-ss` 13 / 17, `-ay` 2 / –, `-np` 30 / 30, `-pc` 92 / 166,
`-pd` 4 / 8, `-sk` 75 / 75, `-as` 7 / 7, plus the four sections `-nidana`,
`-pubbakarana`, `-pubbakicca`, `-pattakalla` in each.

Taken together, the pieces cover SuttaCentral's whole text except one
segment in each Pātimokkha:

- `pli-tv-bu-pm:0.1` and `pli-tv-bi-pm:0.1` are in no piece. In
  SuttaCentral's translation they are empty strings. Their template entry
  (`html/pli/vri/vinaya/pli-tv-bu-pm_html.json`) opens the whole text:
  `<article id='pli-tv-bu-pm'><header><ul><li class='division'>{}</li></ul>`.

No segment is in two pieces (753 distinct segment ids in 753 rows for the
monks', 945 in 945 for the nuns').

**Effect on Simsapa:** Simsapa keeps SuttaCentral's uids
(`pli-tv-bu-pm/en/brahmali`, `pli-tv-bi-pm/en/brahmali`, each one record
with `content_json`), so none of the 546 suttas-core ids matches a Simsapa
record. The import skips them all. That is harmless today, because Simsapa
already has these texts segmented from SuttaCentral, but it means
suttas-core's analysis layers (`sutta_analysis`, `sutta_toc`, `sutta_tags`)
for the Pātimokkhas cannot be joined to Simsapa's records by id.

**Questions:**

1. Is the per-rule split deliberate, e.g. so that each rule gets its own
   `sutta_analysis` / `vinaya_sections` row? Would you consider also keeping
   a record of which SuttaCentral text each piece belongs to (a
   `parent_sutta_id` column, or a mapping in `suttas.json`), so consumers can
   reassemble `pli-tv-bu-pm` / `pli-tv-bi-pm`?
2. Is dropping the `:0.1` segment intended? It is empty in the
   translation, but its template entry carries the opening `<article>` and
   `<header>` markup, so the pieces cannot be reassembled into SuttaCentral's
   document without it.

## 2. `pli-tv-pvr17` (Brahmali, Parivāra 17) is not imported

`build.log` reports:

```
sc-data texts not in suttas.json, skipped: {"brahmali":1,"kovilo":1,"pli":1715,"sujato":118}
```

The Brahmali text is `pli-tv-pvr17`: SuttaCentral has
`translation/en/brahmali/vinaya/pli-tv-pvr/pli-tv-pvr17_translation-en-brahmali.json`,
and suttas-core has `pli-tv-pvr10` … `pli-tv-pvr16` and `pli-tv-pvr18` …
but no `pli-tv-pvr17` in `suttas`.

``` sql
SELECT id FROM suttas WHERE id LIKE 'pli-tv-pvr1%' ORDER BY id;
```

**Question:** is `pli-tv-pvr17` left out on purpose? (And, for completeness:
which 118 Sujato texts and which Kovilo text are skipped, and why?)

## 3. Trailing whitespace is trimmed from some segments

SuttaCentral's Bilara strings end with a space where the next segment
continues the same paragraph; the space is the word separator when the
segments are joined. suttas-core trims it from some segments. In Brahmali's
monks' Pātimokkha, 546 segments differ from SuttaCentral's, all by trailing
whitespace only, e.g.:

| segment | SuttaCentral | suttas-core |
|---|---|---|
| `pli-tv-bu-pm:0.2` | `"The monks’ rules "` | `"The monks’ rules"` |

The same happens in copied SuttaCentral texts elsewhere (45 of 334 segments
of `mn1` Sujato). The translations Simsapa imports (Bodhi, Ṭhānissaro, PTS)
mostly keep a trailing space, and the segments without one that we sampled
were legitimate joins (end of a verse line, after an em dash).

**Question:** is the trimming deliberate, and is it applied only where a
segment ends a block, so that joining a block's segments with no separator
is always correct?

## 4. Ṭhānissaro's introductions are removed

The `than` parser removes the introduction of every page
(`src/sources/than/page.ts`, `bodyStart()`: the `h2.intro` section, or the
run of mostly-`<em>` paragraphs after the `h1`), and skips `sn44`, whose page
is only the saṁyutta's introduction. With the 2026-10-31 data that is the
introduction of 141 texts, about 25,000 words, among them `dn1`, `dn2`,
`dn15`, `dn16`, `dn33`, `mn9`, `mn22`, `mn49`, `mn60`, `mn101`, `mn122`,
`sn1.20` and `thig10`.

These are part of Ajahn Ṭhānissaro's translations as he publishes them.
Simsapa cuts them out of its dhammatalks.org records by the same rule and
keeps them with the segmented records, and keeps `sn44` as an HTML record.

**Question:** could suttas-core keep the introductions, e.g. as a separate
layer keyed by `sutta_id` (HTML or Markdown, with the note cites they
contain), so that its output carries everything on the page?

## 5. Two Ṭhānissaro texts have only empty segments

`thag3.8` and `thag3.9` have 17 `sutta_segments` rows each, every one with
`content = ''`, while their `raw_translations` rows hold the poems (375
characters for `thag3.8`). The segmentation seems to have failed without an
error.

Simsapa does not import a text whose segments are all empty, and keeps its
dhammatalks.org record of `thag3.8` (it has none of `thag3.9`).

**Question:** can the build fail, or report, when a text's segments are all
empty?
