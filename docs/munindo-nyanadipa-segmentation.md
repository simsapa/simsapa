# Munindo, Nyanadipa and SuttaCentral-HTML segment files, Dhammapada verse groups, verse lines

How Ajahn Munindo's *A Dhammapada for Contemplation*, Bhante Nyanadipa's
*The Silent Sages of Old* (Sutta Nipāta), and the three Dhammapada
translations SuttaCentral has only as HTML (Ānandajoti, Buddharakkhita,
Feldmeier) are stored as segmented (Bilara-style) texts, how the Dhammapada
verse-group numbers are added to the Pāli and the English records, and how the
reader renders line breaks and indentation inside a verse segment.

A segmented record (`content_json` keyed by the SuttaCentral segment ids of
the Pāli `ms` text) is shown line by line beside the Pāli in the Lines and
Columns layouts and is served by `/sutta.json`. An HTML-only record is not.

Code:

- `bootstrap_cli/src/bootstrap/segment_files.rs` — file types and readers,
  hashes, the checks of `check-segment-files`, and the import shared by both
  importers (`build_segmented_record()`, `import_source_folder()`).
- `bootstrap_cli/src/bootstrap/dhammapada_munindo.rs`,
  `bootstrap_cli/src/bootstrap/nyanadipa.rs`,
  `bootstrap_cli/src/bootstrap/dhammapada_suttacentral.rs` — the three
  importers, thin wrappers over `import_source_folder()`.
- `bootstrap_cli/src/bootstrap/helpers.rs` — `dhp_verse_group_ranges()`,
  `apply_dhp_verse_ranges()`, `dhp_chapter_number()`, `range_title_key()`.
- `backend/src/helpers.rs` — `render_verse_lines()`.

The decisions behind the placement of every non-obvious line, and every
mismatch between a source file and the Pāli, are in the review reports
`2026-11-08-100734-review---munindo-nyanadipa-segmentation.md` and
`2026-11-15-192449-review---suttacentral-dhammapada-segmentation.md` (in
`tasks/` or `tasks/archive/`).

## 1. Files

Each source folder under `bootstrap-assets-resources/` has a `bilara/` folder:

| Source folder | Segment files | Cut from |
|---|---|---|
| `dhammapada-munindo/` | `bilara/dhp1-20.json` … `dhp383-423.json`, 26, one per chapter | `markdown/dhp-N.md` |
| `nyanadipa-translations/` | `bilara/snp1.1.json` …, 36, one per sutta, plus `bilara/footnotes.json` | `texts-sc-numbering/*.md` |
| `dhammapada-suttacentral/` | `bilara/<translator>/dhp1-20.json` …, 26 per translator (`anandajoti`, `buddharakkhita`, `feldmeier`) | `sc-data/html_text/en/pli/sutta/kn/dhp/<translator>/dhp/*.html` |

The files are data, not generated at bootstrap time. Munindo's and
Nyanadipa's were written by hand (by reading); the SuttaCentral ones are
drafted by a script and hand placements (§8). The `markdown/`, `html/` and `texts-sc-numbering/` files stay as the
input they were made from and are never edited. Munindo's `html/` files are
only pandoc output of his Markdown, so the Markdown is the input. Nyanadipa's
own `texts/` folder (his numbering, shifted in the Pārāyanavagga) is not used.

The file name and the `sutta_id` are the SuttaCentral id, which is not always
the input's name: `texts-sc-numbering/snp5.18.md` is the song of Piṅgiya, SC
`snp5.19`, so its file is `bilara/snp5.19.json` and its record
`snp5.19/en/nyanadipa`. The importers derive the uid from `sutta_id`, never
from `input`.

### Segment file

The segmentation form of suttas-core
(`segmentations/<translator>/<sutta_id>.json`):

```json
{
  "sutta_id": "dhp179-196",
  "translator": "munindo",
  "input": "markdown/dhp-14.md",
  "input_hash": "3f0c…",
  "pali_hash": "a91e…",
  "segments": [
    {"id": "dhp179:0.1", "text": "", "source": "copy"},
    {"id": "dhp179:0.3", "text": "14. The Buddha", "source": "manual"},
    {"id": "dhp179:1", "text": "The Buddha’s perfection is complete;\nthere is no more work to be done.\n", "source": "manual"}
  ]
}
```

- `segments` lists **every** segment id of the Pāli `ms` root file, in the
  Pāli file's order, including the empty ones.
- `input` is the source text, relative to the source folder; for
  `dhammapada-suttacentral`, relative to `sc-data`
  (`html_text/en/pli/sutta/kn/dhp/feldmeier/dhp/dhp1-20.html`).
- `source`: `copy` for a segment taken over unchanged (empty or structural),
  `auto` for text placed by the `dhammapada-suttacentral` drafting script's
  rules, `manual` for text placed by hand. `manual` means "never overwrite
  without asking", as in suttas-core. Any other value is a read error.
- `copyright`, `license` (optional): stored in `suttas.copyright` /
  `suttas.license`. The SuttaCentral files carry them from the page footers;
  the reader does not show them.
- `introduction` (optional, HTML): a chapter- or sutta-level introduction by
  the translator. The importer stores it in `suttas.description` and prepends
  its text to `content_plain`, as for Ṭhānissaro's introductions. Neither
  source has one today; book-wide front matter (prefaces, general
  introductions, copyright pages) is never put here.
- Footnote markers are **not** in the segment text.

### Hashes

Equal to suttas-core's (`src/lib/hash.ts`), so a file can be checked for
staleness the same way:

- `hash(t)`: the first 16 hex digits of SHA-256 of the NFC form of `t`.
- `input_hash` = `text_hash(source text)`: `hash` of the text with `[^n]` and
  `[]` removed, `\s+` collapsed to one space, trimmed. Re-indenting a verse or
  moving a marker does not make a file stale.
- `pali_hash` = `hash` of the ordered Pāli segment ids joined by `\n`.

### Footnote file (Nyanadipa)

Nyanadipa's 29 notes are the book's endnotes, held in his Markdown as `[^n]`
definitions. suttas-core derives footnote placement at build time from the raw
text; these sources have no such build, so the placement is stored in
`bilara/footnotes.json`, one entry per note, with the fields of a suttas-core
`segment_footnotes` row:

```json
{ "footnotes": [
  {"sutta_id": "snp4.2", "segment_id": "snp4.2:3.2", "marker": "1",
   "number": 1, "word_index": 2,
   "content": "“Incorrigible” for <i>avadāniyā</i>. …", "editor": false}
] }
```

- `number`: 1-based reading order within the sutta.
- `word_index`: the 0-based word of the stored segment text the note follows,
  counted as `backend/src/footnotes.rs` counts (split on whitespace, so `\n`
  does not shift it); `null` for a note on the whole segment.
- `content`: the note's Markdown converted to HTML, wording unchanged.

Munindo's chapters have no notes, so his folder has no footnote file.

## 2. Content rules

**Faithful to the author's text.** Joining a file's segment texts in order
reproduces the translator's text line for line, in the translator's order.
The only differences allowed:

- the translator's verse numbers are removed (Munindo's `183.\` lines,
  Nyanadipa's `1\.` prefixes); the verse-group number (§4) replaces them;
- book-wide front matter is not included;
- Markdown becomes the inline HTML the reader uses (`<i>`, `<em>`); curly
  quotes stay; a straight apostrophe after a letter becomes `’`, as the book
  prints it;
- corrections recorded in the review report.

**Placing lines.** A translator's lines are spread over the segments of the
Pāli verse they translate, with `\n` between lines inside a segment:

- Same number of lines as the Pāli verse: one line per segment.
- Different counts: each segment takes the lines closest in meaning to its
  Pāli line, **in the translator's order** even where the Pāli order differs.
  Reading down the column gives the author's verse unchanged.
- Every line is in exactly one segment; none is dropped or repeated.
- A translation verse covering several Pāli verses (Munindo's `186-187.`) is
  split where the meaning passes from one Pāli verse to the next, as Sujato
  and Ṭhānissaro split Dhp 186–187. A split that cannot fall on a sentence
  boundary is listed in the review report.
- **Munindo's verse numbers are not a reference.** He sometimes numbers verses
  singly where the Pāli groups them, and groups others differently from the
  verse stories. Which Pāli verse a line translates is decided from the
  meaning, against the Pāli and Sujato's and Ṭhānissaro's translations, never
  from his numbers.
- Pāli prose segments with no translation (the narrative frame of `snp3.6`)
  stay `""`.
- Trailing whitespace at a segment end separates words when segments are
  joined; keep it.

**Titles.** The translator's title goes in the segment the Pāli template
renders as the page title (`page_title_key()`):

| Source | Segment | Template markup | Value |
|---|---|---|---|
| Munindo | `dhp179:0.3` | `<h1 class='range-title'>` | `14. The Buddha` |
| Nyanadipa | `snp4.2:0.2` | `<h1 class='sutta-title'>` | `The Cave` |
| SuttaCentral HTML | `dhp179:0.3` | `<h1 class='range-title'>` | `14. The Chapter about the Buddha` (the page's `<h1>`) |

The multi-column views show no record title (a column header holds only the
column label), so a title outside a segment is not seen at all; in the Solo
layout the same segment is the page's `<h1>`, so the title appears exactly
once in every layout. The Dhammapada value is the chapter number and the
translator's title, as Sujato writes `14. The Buddhas`. Nyanadipa's value is
the short heading of his Markdown file, which must name the same sutta as the
Pāli title of the SC number. The division segments above the title stay `""`.
The record's `title` field is the title without the chapter number
(`record_title()`), so search results and the navigation tree show
`The Buddha`, `The Cave`, `The Chapter about the Buddha`.

The Dhammapada verse-group heading segments are `""` in the files; the
importer fills them (§4).

## 3. The import

`import_source_folder()` runs after the SuttaCentral, dhammatalks.org and
suttas-core steps, in one transaction per source. Per segment file
(`build_segmented_record()`):

- `content_json_tmpl`: the template of Simsapa's own `<sutta_id>/pli/ms`
  record; a missing Pāli record is an error naming the uid.
- `content_json`: one key per template key, in template order, the file's
  text or `""`, through `IndexMap` and `serde_json::to_string` (never sorted,
  never through `serde_json::Value`). A file id or a note segment the template
  lacks is an error. `consistent_niggahita()` is applied to every segment,
  note and introduction.
- The Dhammapada texts: the verse-group ranges (§4), before the plain text.
- `content_plain` through `bilara_html_and_plain_text()`, without notes, the
  introduction first; `content_html = NULL`; `description` = introduction.
- Notes → `sutta_comments` in the `ssp-fn` stored form, through the same
  `footnotes_value_from()` and `encode_footnotes()` as the suttas-core import
  ([suttas-core-import.md](./suttas-core-import.md) §5). Sutta references in
  a note become links (`link_sutta_refs()`, resolved against the Pāli `ms`
  uids); `Sn 4:17` and `Sn 204` (a verse number) stay text.
- `uid` `<sutta_id>/en/munindo` / `…/en/nyanadipa`; `source_info`
  `dhammapada-munindo/bilara` / `nyanadipa-translations/bilara`.
- The SuttaCentral HTML texts are not inserted: each **replaces the text of
  the HTML record the SuttaCentral import wrote** under the same uid
  (`dhp1-20/en/feldmeier`), in place (`replace_suttacentral_record()`), keeping
  `id`, `uid`, `sutta_ref`, range fields and `source_uid`, and setting the
  title, the content fields, `copyright`, `license` and `source_info`
  `dhammapada-suttacentral/bilara/<translator>`. The step runs after Munindo's.
  A missing record is an error. A record that already has `content_json`
  (SuttaCentral has meanwhile published the text segmented) is kept and
  logged, and its segment file is then obsolete.

All three sources are **required** in the preflight (the `bilara/` folders,
one per SuttaCentral translator, and Nyanadipa's `footnotes.json`), and a
missing or empty folder or any file that fails to build stops the bootstrap,
so a release cannot ship with a text missing. The one exception is a
`--limit` run, which imports only the first N SuttaCentral texts: there a text
whose `…/pli/ms` template or SuttaCentral record to replace was not imported is
skipped with a `--limit: skipping` warning (`skip_under_limit()` in
`bootstrap_cli/src/bootstrap/mod.rs`). A new step that reads records another
step wrote goes through the same function for a missing record, or `--limit`
runs stop at it. Sujato stays the default English beside the Pāli, and the
`source_info` keeps these records behind SuttaCentral's segmented English
where Sujato has none (`default_english_for_pali()`, see
[suttas-core-import.md](./suttas-core-import.md) §2).

## 4. Dhammapada verse groups

Every verse group of the Dhammapada shows its verse range: the Pāli heading
reads `183–185. Ānandattherapañhavatthu`, and the same segment in the English
records reads `183–185.`, so the number stands above the group in every column.

**The rule** (`dhp_verse_group_ranges()`), from the segment ids alone (the
template, the Pāli root and every translation of a chapter share them):

- A heading is a `dhpN:0` id, or the last `dhpN:0.x` id (`x` ≥ 4) of the
  chapter's first verse (`dhp179:0.4`; `0.1`–`0.3` are collection, book and
  chapter title).
- A group runs from its heading's verse to the verse before the next heading,
  or to the chapter's last verse: `dhp183:0` → `183–185`, `dhp179:0.4` →
  `179–180`.
- One verse is a single number (`181`); the separator is an en dash (U+2013).
- The ranges of the 26 chapters cover 1–423 without gap or overlap (tested).
- Exception: `dhp416:5.0` (*Jotikattheravatthu*, the Pāli root repeating
  Dhp 416 for a second story) is a verse-story heading but not of the heading
  form, and gets no range. Count headings by this rule, never by
  `LIKE '%:0%'`.

**Writing it** (`apply_dhp_verse_ranges()`), before `content_plain` is
computed so the plain text carries it:

- Pāli `…/pli/ms`: `"<range>. <Pāli text>"`, keeping the text's trailing space.
- Translation: exactly `"<range>."`. A heading that already held text is
  overwritten and logged with uid, segment id and text. Known case:
  Ṭhānissaro's `dhp239:0` held `235-238*` (a suttas-core defect, listed in the
  review report).

Applied by three importers, and only to these records:

| Records | Importer |
|---|---|
| `dhp…/pli/ms`, `dhp…/en/sujato` | SuttaCentral, `bilara_text_to_sutta()` (`suttacentral.rs`) |
| `dhp…/en/thanissaro` | suttas-core, `build_record()` (`suttas_core.rs`) |
| `dhp…/en/munindo`, `dhp…/en/anandajoti`, `…/en/buddharakkhita`, `…/en/feldmeier` | `build_segmented_record()` (`segment_files.rs`) |

The other Dhammapada translations (other languages, and the English ones
stored as HTML) and the Sutta Nipāta get no ranges.

The suttas-core importer also writes `<chapter>. <title>` into the
range-title segment of Ṭhānissaro's 26 chapters (`14. Awakened`): his title
from that segment, trimmed, else the record title; the chapter number from
`dhp_chapter_number()` (`DHP_CHAPTERS_TO_RANGE`), never parsed from a title.

## 5. The check command

```sh
cd bootstrap_cli
cargo run -- check-segment-files dhammapada-munindo
cargo run -- check-segment-files nyanadipa-translations [--sc-data <dir>]
cargo run -- check-segment-files dhammapada-suttacentral   # all three translators
```

A folder name that is not a path is looked up in `bootstrap-assets-resources`;
`--sc-data` defaults to its `sc-data`. Read-only. **Run it after writing or
editing any segment file**; the bootstrap does not run it. For every file:

1. the segment ids are exactly the Pāli root ids, in order;
2. the joined text equals the normalised source (`normalized_source_text()`:
   title, verse numbers, footnote markers and definitions, Markdown line-break
   syntax, emphasis and escapes removed; HTML tags removed from the segments;
   whitespace collapsed; apostrophes curly on both sides; for a SuttaCentral
   page, `normalized_suttacentral_html()`: the text between `</header>` and
   `<footer>` without the `SC n` verse anchors and tags); the first differing
   word is printed with its context and segment id;
3. `input_hash` and `pali_hash` are current, else the file is "stale";
4. the segments above the title and the verse-group headings are empty, and
   the title segment holds the source title (`14. ` prefix for Munindo);
5. every `[^n]` of the source is in `footnotes.json` exactly once, in reading
   order, on a segment of that text, at a word the segment has, with the
   definition's wording, and no note lacks a source marker;
6. every source text is the `input` of exactly one segment file, so the files
   together give back the whole original.

It prints every problem and a summary, and exits 1 on any problem.

## 6. Line breaks and indentation in verse segments

The segmented data keeps a translator's verse lines as `\n` inside a segment
(Ṭhānissaro, Bodhi, Munindo, some of Nyanadipa), and suttas-core carries
Ṭhānissaro's indentation as leading tabs (built with `--verse-indent`, see
[suttas-core-import.md](./suttas-core-import.md) §1).

**The segment-text contract** (shared with suttas-core and stated in the API
doc):

- `\n` separates lines. A `\n` at the end of the segment, with any whitespace
  after it, ends the last line and renders nothing.
- A line may start with tabs; their number is its indent level, 0 being flush.
  A tab elsewhere is ordinary whitespace.

**Rendering** (`render_verse_lines()`): only in a **verse segment**, one whose
template entry contains `class='verse-line'`. Each line becomes
`<span class='ssp-line ssp-indent-N'>…</span>`, the spans separated by the
`\n` (so the page text keeps a break between lines: the find bar and copied
text read `He is one who`, not `He isone who`), and the whitespace after the
last line is kept after the last span. A verse segment without an inner `\n`
or a leading tab is returned unchanged, byte for byte, so Sujato, the Pāli and
every SuttaCentral translation render as before. Prose segments are not
touched: a `\n` in prose collapses into a space.

It is called in one place, `bilara_text_to_segments()`, after the footnote
markers are inserted (so a marker stays in its word's line) and before the
variant and gloss marks. The Solo layout and the multi-column path both go
through it (`wrap_in_template` says whether the segment is wrapped in its
template entry), so all layouts show the same lines.

**CSS** (`assets/sass/_suttacentral.sass`): `span.ssp-line` is
`display: block` with a 2em hanging indent for a line too long for its column;
`ssp-indent-N` adds N em. Ṭhānissaro's `p.vN` on dhammatalks.org is level N,
so his `v2` lines sit 2em in, `v4` lines 4em.

The stored text is not changed: `content_json`, `/sutta.json` and
`/sutta_bilara.json` keep `\n` and `\t`; `content_plain` treats them as
whitespace.

## 7. Traps

- Never derive the Dhammapada alignment from Munindo's verse numbers.
- Never hand-edit a `dhammapada-suttacentral` segment file: the drafting
  script rewrites them all. A placement decision goes in
  `tools/placements.json` (§8).
- Never sort `content_json` keys, and keep every template key.
- Keep trailing whitespace at segment ends.
- Leave heading segments `""` in the files; the importer writes the ranges.
- Select verse-group headings by the rule of §4, not by `:0`.
- Footnote markers go in before `render_verse_lines()`, never after.
- A change to a segment file or to how ranges are written changes
  `content_plain`: it ships with a re-bootstrap and an index rebuild.

## 8. The SuttaCentral HTML Dhammapada translations

SuttaCentral has Ānandajoti's, Buddharakkhita's and Feldmeier's Dhammapada
only as `html_text` pages, one per chapter, with an anchor before each verse
(`<a class='ref sc' id='sc183' … data-uid='dhp183'>SC 183</a>`). The files in
`dhammapada-suttacentral/bilara/<translator>/` are written by
`tools/draft_segment_files.py` in that folder, from those pages, the Pāli root,
and `tools/placements.json`:

```sh
cd bootstrap-assets-resources/dhammapada-suttacentral
python3 -I tools/draft_segment_files.py .. [--report <file.json>]
```

It rewrites every file, so the files can be redone after an `sc-data` update
(a page changed by SuttaCentral shows as "stale" in check 3) without losing a
hand placement. Its rules:

- **Verses** come from the anchors' `id`, never `data-uid` (wrong, `dhp1`, on
  every verse of Ānandajoti's chapter 23). Anchors with no text between them
  (Buddharakkhita's Dhp 87–88, 104–105, 138–140, 186–187, 190–191, 195–196,
  246–247, 271–272, 345–346) form one group: his paragraph translates them
  together, and its sentences are spread over all their segments.
- **Units** are the translator's lines (`<br>`, paragraph ends), kept as `\n`
  inside a segment. Buddharakkhita writes prose: his units are sentences and
  clauses (split after `. ; : ? !` and a space), joined by a space inside a
  segment. Feldmeier's pages keep the print edition's **line wraps**
  (`following death he is<br>tormented;`): a Feldmeier line that starts in
  lowercase stays one unit with the line before it, the break kept.
- **Placement**, in the translator's order, as §2: as many units as Pāli lines,
  one each; Ānandajoti with half as many, one line per half-verse (`dhp183:1`,
  `dhp183:3`; his lines are the Pāli half-verses); otherwise a monotone
  alignment by meaning, scored by the words a unit shares with Sujato's,
  Suddhaso's, Ṭhānissaro's and Munindo's segments. The alignment is a draft:
  every verse it placed was read, and the ones it got wrong are fixed in
  `tools/placements.json` (`"feldmeier/dhp5": ["dhp5:1", "dhp5:3", "dhp5:4"]`,
  `source: manual`).
- **Across an anchor.** Where SuttaCentral anchors a line to the verse next to
  the one it translates (Feldmeier's Dhp 229–230 and 375–376), a placement key
  naming the range (`feldmeier/dhp229-230`) joins the groups, and the line is
  placed in the verse it translates. Each such case is in the review report.
- **Left empty**: the verse-group headings (the importer writes the ranges),
  each chapter's vagga-end line (`Yamakavaggo paṭhamo.`), the colophon of
  Dhp 423, and the repeat of Dhp 416 (`dhp416:5.1`–`8`).
- **Not text**: the page header (division names) and footer (translator,
  edition, licence). The footer's holder and licence are in the files'
  `copyright` / `license`.

After a run, `check-segment-files dhammapada-suttacentral` must report no
problem.
