# Dhammapada commentary: Ānandajoti's pages, and the commentary records of a vagga

Bhikkhu Ānandajoti's *Dhamma Verses Commentary: A Revised Translation of the
Dhammapada Aṭṭhakathā (Revised 2024)* is imported as one sutta page per
Dhammapada vagga, `dhp<start>-<end>.att/en/anandajoti`. It sits beside the
CST aṭṭhakathā of the same vagga and its English translations:

| Record | Source |
|---|---|
| `dhp21-32.att/pli/cst` | CST aṭṭhakathā (`s0502a.att.xml`) |
| `dhp21-32.att/en/opentipitaka` | OpenTipitaka English of it |
| `dhp21-32.att/en/anandajoti` | Ānandajoti, this page |
| `dhp21-32.att/en/daw` | Daw Mya Tin, tipitaka.net (§8) |

The same book stays in the library as `dhp-comm`
(`library-imports/library-imports.toml`), with its Introduction, Prologue
and Conclusion. The sutta pages hold the stories only.

Code: `bootstrap_cli/src/bootstrap/dhammapada_commentary_anandajoti/`
(`extract.rs` the vagga / story structure and checks, `page.rs` the page HTML
and its class tables, `import.rs` the bootstrap step), on the shared epub code
in `bootstrap_cli/src/bootstrap/anandajoti_epub/` (epub reading, the block
stream and notes, the HTML writer, the page frame, the report, `CopiedColumns`
and the SHA-256 check), which his Jātaka pages use too
([jataka-commentary-anandajoti.md](./jataka-commentary-anandajoti.md) §8).
Styles: `assets/sass/_anandajoti.sass`.

## 1. The uid scheme

The unit is the SuttaCentral vagga uid. The CST Dhammapada files
(`s0502m.mul.xml`, `s0502a.att.xml`) are on the verified list, so the
bootstrap imports them as fragments coded with those uids: `dhp21-32/pli/cst`
for the mūla and `dhp21-32.att/pli/cst` for the aṭṭhakathā, each with its
`/en/opentipitaka` record ([sutta-navigation-tree.md](./sutta-navigation-tree.md)
§3.1). Their record header reads "Dhammapada 21-32" and
"Dhammapada-aṭṭhakathā 21-32" (`header_nikaya_and_number()` in
`tipitaka_xml.rs`).

Every commentary page uses the same reference, `dhp<start>-<end>.att`, so the
existing matching links them with no Dhammapada-specific code:
`split_sutta_ref_kind()` reads the `.att` suffix, `related_sutta_uids()`
relates the verse ranges, and `pali_edition_twins` pairs `dhp21-32/pli/ms`
with `dhp21-32/pli/cst` (never an `.att` record).

The page's uid comes from the **SuttaCentral range of its vagga number**, not
from its stories' `Dhp` lines. Two of those lines are wrong in the source
(§6), and would otherwise name the pages `dhp235-254` and `dhp40-423`, which
match no CST record.

## 2. Commentary in the translation tabs and in search

**A record is a commentary when its reference ends in `.att` or `.tik`,
whatever its source.** `dhp21-32.att/en/anandajoti` and
`dhp21-32.att/en/daw` are commentaries exactly as `dhp21-32.att/pli/cst` is.

- **Translation tabs** (`related_sutta_uids()`): opening `dhp21-32/pli/ms`
  lists the `.att` records only with *Include commentary in translations*
  (`include_cst_commentary_in_translations`). Opening `dhp21-32.att/pli/cst`
  lists the other `.att` records of the vagga whatever the setting, since
  they are the same kind as the opened text. The kind table is in
  [sutta-navigation-tree.md](./sutta-navigation-tree.md).
- **Search** follows *Include commentary in search results*
  (`include_cst_commentary_in_search_results`) in all three paths. Contains
  Match excludes `uid LIKE '%.att%/%' OR uid LIKE '%.tik%/%'`
  (`COMMENTARY_UID_LIKE_PATTERNS` in `query_task.rs`, shared by the Diesel
  filter and the FTS5 `extra_where`). Fulltext excludes the Tantivy
  `is_commentary` field, set in `indexer.rs` from `.att` / `.tik` before the
  first `/`. The `%.att%` form also matches the division uids of unverified
  CST files (`s0501a.att2.xml/pli/cst`).
- The CST mūla setting (`include_cst_mula`) is CST-only and separate:
  `dhp21-32/pli/cst` follows it, `dhp21-32/en/anandajoti` (the verses) never.

> **Trap: a filter written as `'%.att%/cst'` or keyed on the source hides the
> CST commentary and shows the others.** The setting names still say "cst"
> for history; the rule is by uid.

The column-bar dropdown labels by author and edition only, so on
`dhp21-32/pli/ms` the Ānandajoti verses and the Ānandajoti commentary both
read "anandajoti (en)". The tabs show the full uid. The page has no
`content_json`, so it is disabled in the Lines layout (`has_content_json`
false).

**Gloss references.** The CST Dhammapada records are keyed by vagga, so a
gloss of `dhp23` reads `dhp21-32/pli/ms` by default (see
[simsapa-cli-and-localhost-api.md](./simsapa-cli-and-localhost-api.md) §16.1).

## 3. Extract, then import

**Extract** (run by hand, and again only when the epub changes):

```sh
cd bootstrap_cli && cargo run --release -- dhammapada-commentary-extract
# --epub <path>   default: bootstrap-assets-resources/library-imports/books/Dhamma-Verses-Comm.epub
# --output <dir>  default: bootstrap-assets-resources/dhammapada-commentary-anandajoti/
```

It needs no database. It writes, overwriting:

- `dhp<start>-<end>.html` per vagga: the whole `content_html` of the record;
- `manifest.json` (pretty-printed, it is a source file): the epub's name and
  SHA-256, and per vagga `uid`, `file`, `vagga_number`, `title`,
  `title_pali`, `verse_start`, `verse_end`, `story_count`;
- `extract-report.txt`: the defects (§6) and a summary line.

The output is byte-for-byte the same on every run over the same epub. It
fails only when it cannot read the epub or write the files, never on a source
defect.

**Import** (`DhammapadaCommentaryAnandajotiImporter`, in `bootstrap()` after
the Munindo / Nyanadipa step and before the navigation trees and the fulltext
indexes). One `suttas` row per manifest entry:

- `sutta_ref`, `nikaya`, `group_path`, `group_index`, `order_index` and the
  `sutta_range_*` columns are copied from the `.att/en/opentipitaka` row of
  the same vagga (`CopiedColumns`), which the CST import has just written.
  Without it, they are derived from the uid and a warning is logged.
- `language` `en`, `source_uid` `anandajoti`, `source_language` `pli`;
  `title` the English vagga title, `title_pali` the vagga name;
  `content_json` NULL; `copyright` "Ānandajoti Bhikkhu", `license`
  "CC BY-SA 4.0", `source_info` naming the book and its URL.
- `content_plain` is `sutta_html_to_plain_text()` of the page after
  `page_plain_text()` removes the Notes section, the note-ref numbers
  (otherwise "body.102 But" is indexed) and the `<nav>` lists (they repeat
  every heading).

The folder's `manifest.json` is a **required** preflight source
(`bootstrap_sources()`). The importer compares the manifest's SHA-256 with
the epub in `library-imports/books/`; on a mismatch, or with no epub, it
logs a warning saying to re-run `dhammapada-commentary-extract`, and goes on.

> **The `nikaya` differs from the verse records.** The pages copy `khuddaka`
> from the OpenTipitaka row; the `dhp*/en/anandajoti` verse records have
> `dhp`. The search's nikāya filter treats them differently.

## 4. The epub, as the extraction reads it

The full survey is the module doc of `dhammapada_commentary_anandajoti/mod.rs`.
What the rules depend on:

- **Spine** (`Ops/content.opf`): `001.html`–`039.html`, then `notes.html`.
  `001`–`006` are front matter (the Prologue, Ganthārambhakathā, is an `<h2>`
  in `006`). The vaggas run from `007` to `037`. `038.html` holds only
  `<h1>&nbsp;</h1>`; the Conclusion (Nigamanakathā) is `039.html`.
  `Ops/toc.ncx` nests vagga → story → sub-story, and each heading is checked
  against it.
- **Content is assigned by heading order, never by file.** A file may begin
  inside a story (`015.html` at the sub-story "15b The Novice Tissa").
- **Vagga:** an `<h1>` numbered `N. `, English and Pāli split by `<br/>`
  (by ", " in vaggas 8, 23, 25 and 26). An empty or unnumbered `<h1>` never
  starts a vagga.
- **Story:** an `<h2>` numbered `<vagga>.<n> ` (305; its class varies, so the
  number is matched), then its Pāli title (`<h3 class="h30">`), then the
  metadata paragraphs (`p7`, once `p14`): the `Dhp` verse line, Burlingame's
  title, parallels, a summary, the cast, keywords and a star rating.
- **Sub-story:** an `<h4 class="h40">` (185), labelled `1a`, `1a.`, `Part 5.`
  or not at all.
- **Note ref:** `<a href="notes.html#aNNNN"><span …>{N}</span></a>`, with nine
  span classes, some inside `<h4>`: recognised by its `href`, never its
  class. Notes are `<p class="p16">` paragraphs in `notes.html`, each opening
  with its anchor and a back-link (one note runs on into a second `p16`).
- **Page markers:** grey spans (`t10`, `t17`, `t21`, `t25`), `{1.3}` (PTS) and
  `[28.146]` (Harvard Oriental Series), often both in one span, sometimes
  inside a verse line.
- The Introduction, Abbreviations, Prologue and Conclusion are **not written
  to any page**. The Pāli records differ at both ends: the parser attaches
  every passage to some uid, so the Pāli Ganthārambhakathā is inside
  `dhp1-20.att/pli/cst` and the Nigamanakathā and colophon inside
  `dhp383-423.att/pli/cst`. The first and last English pages are shorter
  there by design.
- The Conclusion's per-vagga story counts ("In the Chapter about the Pairs are
  fourteen stories.", "… there are nine.") are parsed only to check the
  counts.

## 5. The page

Framed like the other `/en/anandajoti` records
(`div.suttacentral.html-text > article > header`), with the vagga title as
the `<h1>`, then inside `div.anandajoti-dhp-comm`:

1. `nav.page-toc`: one entry per story, "Dhp 1 — 1.1 The Story about …";
2. per story, the verse anchors, then `section.story` with its heading,
   `h3.story-title-pali`, `div.story-meta`, `nav.substory-toc` (only when it
   has sub-stories) and the text;
3. `section.notes`: the notes this page refers to, in order of first
   reference, with **the book's note numbers**.

**Anchors** (unique within the page):

| What | id |
|---|---|
| story | `story-1-1` |
| sub-story | `story-1-1-1a`, `story-2-1-part-5`, or `<story id>-sub-<k>` with no usable label |
| verse | `dhp-v5`, one empty `<a>` per verse before its story (only before the first story on the page that names it) |
| note ref | `note-ref-4`; later refs to the same note `note-ref-4-2`, … |
| note | `note-4`, with a back-link (`a.footnote-backref`) to its first ref |

A taken id gets `-2`, `-3`, …. The TOC and sub-story link texts leave out the
heading's note refs and page markers, so no ref is duplicated.

> **Verses covered by no story have no `dhp-vN` anchor** (Dhp 23, 34,
> 228–230, 255, 400), and `dhp-v40` is on two pages: vagga 3's, and vagga
> 26's before story 26.17, whose `Dhp 40` is wrong in the source. A
> jump-to-verse feature must allow for both.

Note refs use the app's footnote markup (`sup.footnote-reference > a`), so
the footnote pop-up and bottom bar work on them; the pop-up strips the
back-link (`get_footnote_content()` in `src-ts/footnote_modal.ts`).

**Class mapping.** The epub's presentation classes are dropped; the
semantic ones are styled in `_anandajoti.sass` with theme-neutral opacity and
translucent rules, so one set works in both themes.

| Epub | Page |
|---|---|
| `p3`, `p5`, `p13` prose, `p7`/`p14` metadata | plain `<p>` (metadata inside `div.story-meta`) |
| `p6`, `p10`, `p11` verse, lines split by `<br/>` | `p.verse` |
| `p8` story of the past | `p.story-past` |
| `p9`, `p12`, `p15`, and the green `t16` span | `.commentator-note` |
| `h3.h30` | `h3.story-title-pali` |
| `h4.h40` | `h4.substory-heading` |
| italic `t6`, `t18`, `t23`, `th43`; bold `t5`, `t14`, `t28`; `t11`; superscript `t22` | `<i>`, `<b>`, `<b><i>`, `<sup>` |
| grey page-marker spans | `span.pts-ref` |
| size / colour-only spans | their text |

Markup not in these tables is listed in the report as unmapped and rendered
as its text.

**Page markers** (`split_page_marker()`): the `pts-ref` span covers only the
text from the first `{`/`[` to the last `}`/`]`. A dialogue dash inside the
source's grey span (`[28.167] –`) is written after the span, unchanged. A
marker split over two spans in the source (`{3.` and `465}`) stays two
`pts-ref` spans.
`{M. ii. 97}` … `{M. ii. 105}` are real markers (PTS Majjhima vol. ii, in the
quoted Aṅgulimāla Sutta).

**Links** to another file or vagga become plain text: no link on a page leads
off it. A test checks that every `href="#…"` has its `id`.

## 6. The defect report

**Source defects are reported, never repaired**, and the text is kept
verbatim, numbering such as "33c." included. `extract-report.txt` lists:

- a verse covered by no story, or by two;
- a story's verses outside its vagga's range, and a vagga whose stories do not
  cover its SuttaCentral range;
- a story with no `Dhp` line, or an unreadable one;
- a note ref with no note, or whose number differs from its note's;
- a story or vagga out of order;
- a story count that differs from the Conclusion, or a count line naming
  another chapter;
- a heading that does not match `toc.ncx`, or a vagga heading with no `<br/>`;
- a sub-story labelled for another story (`14a` in story 14.5).

For the 2024 epub: 26 vaggas, 305 stories, 185 sub-stories, 408 notes, 15
defects. The list is in `extract-report.txt`.

## 7. Tests

- `dhammapada_commentary_anandajoti::tests`: a fixture epub folder
  (`bootstrap_cli/tests/data/dhammapada-commentary-anandajoti/Fixture-Comm.epub/`,
  with `expected/` beside it) compared byte for byte, run twice, plus the link
  checks. `DHP_COMM_BLESS=1` rewrites `expected/`. A real-data test runs on
  the epub when it is present.
- `import.rs` tests: the copied columns, the fallback, the plain text.
- `backend/tests/test_related_sutta_uids.rs`: the `dhp21-32` tabs, pure and
  against the shipped database; `query_task::commentary_filter_tests` and the
  `searcher.rs` filter tests: the commentary rule in each search path.

## 8. Daw Mya Tin's pages are commentary too

Daw Mya Tin's "Dhammapada Verses and Stories" (tipitaka.net) gives each verse
with its story, so it is imported the same way, by `dhammapada_tipitaka.rs`.
The exported database (`dhammapada-tipitaka-net-export`) keeps the legacy
`dhp<range>/en/daw` uids; the bootstrap writes `dhp<range>.att/en/daw`, copies
the grouping columns from the `.att/en/opentipitaka` row (`CopiedColumns`),
and takes the title from the `dhp<range>/pli/ms` row (`Yamakavagga`), because
the source titles every page "Dhammapada".
