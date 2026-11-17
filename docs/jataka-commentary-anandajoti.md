# Jātaka commentary: Ānandajoti's pages, `ja<N>.att/en/anandajoti`

Bhikkhu Ānandajoti's *The Birth Stories (Jātakatthavaṇṇanā) (Revised 2023)*
is imported as one sutta page per Jātaka, `ja<N>.att/en/anandajoti`. It sits
beside the CST aṭṭhakathā of the same Jātaka and its English translation:

| Record | Source |
|---|---|
| `ja62.att/pli/cst` | CST aṭṭhakathā |
| `ja62.att/en/opentipitaka` | OpenTipitaka English of it |
| `ja62.att/en/anandajoti` | Ānandajoti, this page |
| `ja62/pli/ms`, `ja62/en/sujato` | the mūla and its translation |

The same book stays in the library as `jataka-comm`
(`library-imports/library-imports.toml`), with its Introduction, Studies,
Bibliography, Abbreviations, the dates essay and the Nidānakathā. The sutta
pages hold the 547 Jātakas only.

Code: `bootstrap_cli/src/bootstrap/jataka_commentary_anandajoti/`
(`extract.rs` the structure and checks, `page.rs` the page HTML, `import.rs`
the bootstrap step), on the shared epub code in
`bootstrap_cli/src/bootstrap/anandajoti_epub/` (§8). Styles:
`assets/sass/_anandajoti.sass`, under `.anandajoti-jataka-comm` beside the
Dhammapada pages' rules.

The commentary rule, the extract → import stages, the footnote markup and the
defect-report rule are shared with the Dhammapada pages
([dhammapada-commentary-anandajoti.md](./dhammapada-commentary-anandajoti.md));
this doc gives the Jātaka's own facts and rules.

## 1. The uid scheme

The unit is the CST Jātaka-commentary reference. The CST import writes
`ja<N>.att/pli/cst` and `ja<N>.att/en/opentipitaka` for each of the 547
Jātakas (numbers 1–547, contiguous), so the page takes the same reference with
its own source: `ja<N>.att/en/anandajoti`.

`N` is the Jātaka number of the epub's heading (`Ja 62 Aṇḍabhūtajātaka`). It is
cross-checked three ways, and a disagreement is reported, never repaired:

- the number runs 1–547 once each, in order;
- the heading matches the `Ja N …` entry of `toc.ncx`;
- the CST record `ja<N>.att/pli/cst` exists (checked by the importer, not the
  extractor, which needs no database).

The English title is the epub's (`The Birth Story about Being Inexperienced`),
not OpenTipitaka's (`[62] 2. Commentary on the Aṇḍabhūta Jātaka`), and the Pāli
title is the heading's name (`Aṇḍabhūtajātaka`) as `title_pali`.

There is no page for the Nidānakathā or any of the front matter (it stays in
the library book), and none for a Book (Nipāta) heading.

## 2. Commentary in the translation tabs and in search

**A record is a commentary when its reference ends in `.att` or `.tik`,
whatever its source.** `ja62.att/en/anandajoti` is a commentary exactly as
`ja62.att/pli/cst` is. The rules are in
[dhammapada-commentary-anandajoti.md](./dhammapada-commentary-anandajoti.md) §2;
for the Jātaka uids:

- **Translation tabs** (`related_sutta_uids()`): opening `ja62/pli/ms` lists
  the `.att` records only with *Include commentary in translations*. Opening
  `ja62.att/pli/cst` lists `ja62.att/en/anandajoti` and
  `ja62.att/en/opentipitaka` whatever the setting, since they are the same kind
  as the opened text. A neighbouring Jātaka (`ja63.att/…`) never matches, and
  `ja6.att/…` never matches `ja62`.
- **m | a | t buttons** (`related_text_lookup()`): a new `.att/en/anandajoti`
  candidate does not change which record a button opens — from `ja62/pli/ms`
  the aṭṭhakathā (`a`) is still `ja62.att/pli/cst`.
- **Search** follows *Include commentary in search results* in all three paths
  (the Diesel filter, the FTS5 `extra_where` and the Tantivy `is_commentary`
  field), on the same `%.att%/%` / `%.tik%/%` rule. The 547 pages are indexed
  for Contains and Fulltext as 547 `en` documents.
- **Bottom bar** (`option_text()` in `src-ts/column_bar.ts`): the page is
  "anandajoti (en, comm)". It has no `content_json`, so it is disabled in the
  Lines layout (`has_content_json` false).

> **The `nikaya` is the CST commentary's, not the mūla's.** The pages copy
> `khuddaka` from the `ja<N>.att/en/opentipitaka` row; the mūla `ja62/pli/ms`
> has `ja`. The nikāya filter therefore treats a page as the CST rows are
> treated, and differently from the mūla series.

**Gloss references** are unaffected: a `ja62` reference reads `ja62/pli/ms`
(the CST range rule that redirects a `dhp` reference applies to `dhp` only).

## 3. Extract, then import

**Extract** (run by hand, and again only when the epub changes):

```sh
cd bootstrap_cli && cargo run --release -- jataka-commentary-extract
# --epub <path>   default: bootstrap-assets-resources/library-imports/books/The-Jataka.epub
# --output <dir>  default: bootstrap-assets-resources/jataka-commentary-anandajoti/
```

It needs no database. It writes, overwriting:

- `ja<N>.html` per Jātaka: the whole `content_html` of the record;
- `manifest.json` (pretty-printed, it is a source file): `epub_file`,
  `epub_sha256`, and per Jātaka `uid`, `file`, `jataka_number`, `title`,
  `title_pali`, `nipata_number`, `nipata`, `verse_count`;
- `extract-report.txt`: the defects (§6) and a summary line.

The output is byte-for-byte the same on every run over the same epub. It fails
only when it cannot read the epub or write the files, never on a source
defect.

**Import** (`JatakaCommentaryAnandajotiImporter`, in `bootstrap()` after the
CST import and the Dhammapada commentary importer, before the navigation trees
and the fulltext indexes). One `suttas` row per manifest entry:

- `sutta_ref`, `nikaya`, `group_path`, `group_index`, `order_index` and the
  `sutta_range_*` columns are copied from the `ja<N>.att/en/opentipitaka` row
  (`CopiedColumns`). Without that row they are derived from the uid and a
  warning is logged. A manifest entry with no `ja<N>.att/pli/cst` row is
  skipped with a warning: a page is always the translation of a commentary
  record, never given a made-up uid.
- `language` `en`, `source_uid` `anandajoti`, `source_language` `pli`;
  `title` and `title_pali` from the manifest; `content_json` NULL;
  `copyright` "Ānandajoti Bhikkhu", `license` "CC BY-SA 4.0", `source_info`
  naming the book and
  `https://ancient-buddhist-texts.net/English-Texts/Jataka/`.
- `content_plain` is `sutta_html_to_plain_text()` of the page after
  `page_plain_text()` removes the Notes section, the note-ref numbers
  (otherwise "body.102 But" is indexed) and the `<nav>` lists (they repeat the
  headings).

The folder's `manifest.json` is a **required** preflight source
(`bootstrap_sources()`). The importer compares the manifest's SHA-256 with the
epub in `library-imports/books/`; on a mismatch, or with no epub, it logs a
warning saying to re-run `jataka-commentary-extract`, and goes on.

## 4. The epub, as the extraction reads it

The full survey is the module doc of `jataka_commentary_anandajoti/mod.rs`.
What the rules depend on:

- **Spine** (`Ops/content.opf`): 122 content files, `001`–`122.html`, then the
  three notes files `notes.html`, `notes2.html`, `notes3.html`. `001`–`015`
  are front matter (title page, Introduction, Studies, Bibliography,
  Abbreviations, the dates essay, the Nidānakathā). The Jātakas start in
  `019.html` and the epub ends inside Ja 547: there is no back matter.
- **Content is assigned by heading order, never by file.** Seven Jātakas cross
  a file boundary (270, 536, 537, 543, 545, 546, 547).
- **Nipāta:** the `h2` heading `Book II. Dukanipāta  The Section with Two
  Verses` (from Book XII on it ends in the number range "(464-473)"). A Jātaka
  belongs to the last Book heading before it.
- **Jātaka:** an `h3` `Ja 62 Aṇḍabhūtajātaka{note}<br/>The Birth Story about …
  (1s)`, exactly one `<br>`: the Pāli name before it, the English title after.
  Any `h1`, `h2`, `h5`, or an `h3` that is not a Jātaka ("Preface"), ends it;
  the blocks up to the next Jātaka (Book introductions, Prefaces) are skipped.
- **Section:** an `h4` (99, in ten Jātakas: 31, 521, 536–539, 543, 545–547).
  Nothing else divides a Jātaka: the Story of the Present, the Story of the
  Past, the Pāli verses, Ānandajoti's verse translation and the Connection are
  paragraphs, not headings.
- **The section label in the heading** — `(1s)`, `(Pak)`, `(Mahānipāta)`,
  sometimes after the Pāli name — is left out of `title`. It is unreliable as a
  Nipāta (507 headings carry a `<digits>s` label, 16 carry none), so it is only
  cross-checked; the Book heading gives the Nipāta.
- **Verses** are numbered per Jātaka, "N. " opening the Pāli verse paragraph
  (`p10`, also `p21`/`p22`), and the numbering restarts in each section.
  Ja 341 has no numbered verse. Numbers repeat in 35, 220, 392, 536, 541 and
  546, and have gaps in 27 Jātakas: all are reported (§6), and the anchors
  de-duplicated.
- **Notes:** 1766 notes numbered 1–1766 continuously across the three files
  (538 / 873 / 355), one `<p class="p13">` each, opening with
  `<a id="aNNNN"></a><a href="NNN.html#aN"><span>{N}</span></a>` whose link
  returns to the content file. Note refs occur inside headings too. Every note
  is referenced once, but only 1582 from the Jātaka pages: the other 184 are
  cited in the front matter the pages leave out, so a page's Notes section is
  what that Jātaka cites, not the whole book.

## 5. The page

Framed like the other `/en/anandajoti` records
(`div.suttacentral.html-text > article > header`). The `<article>` is
`ja<N>.att`; the header lists `Minor Collection`, `The Jātaka Commentary` and
the Nipāta (`<li class="nipata">Book I. Ekanipāta</li>`), then the `<h1>` with
the English title and `<p class="jataka-title-pali">` with the Pāli name.
Inside `div.anandajoti-jataka-comm`:

1. `section.jataka` with `h2.jataka-heading` ("Ja 62 Aṇḍabhūtajātaka" / the
   English title after a `<br>`, with the heading's note refs);
2. `nav.section-toc`, only when the Jātaka has two or more sections;
3. the paragraphs, with `h3.section-heading` for each `h4` of the source;
4. `section.notes`: the notes this page refers to, in order of first
   reference, with **the book's note numbers** (`<li value="N">`).

**Anchors** (unique within the page; a taken id gets `-2`, `-3`, …):

| What | id |
|---|---|
| the Jātaka | `ja-62` |
| a section | `ja-536-the-small-wood` (slug of the heading's ASCII text), `ja-<N>-section-<k>` when the heading slugs to nothing |
| a verse | `ja-62-v1`, an empty `<a>` before the numbered Pāli verse paragraph; a repeated number gives `ja-536-v1-2` |
| note ref | `note-ref-4`; later refs to the same note `note-ref-4-2`, … |
| note | `note-4`, with a back-link (`a.footnote-backref`) to its first ref |

A verse the source does not number (Ja 341 has none) has **no** verse anchor.
The page TOC's link text leaves out the heading's note refs and page markers,
so no ref is duplicated.

Note refs use the app's footnote markup (`sup.footnote-reference > a`), so the
footnote pop-up and bottom bar work on them; the pop-up strips the back-link
(`get_footnote_content()` in `src-ts/footnote_modal.ts`).

**Class mapping.** The epub's presentation classes are dropped; the semantic
ones are styled in `_anandajoti.sass` with theme-neutral opacity and
translucent rules, so one set works in both themes.

| Epub | Page |
|---|---|
| `p3`, `p4`, `p5`, `p7`, `p20` | plain `<p>` |
| `p10`, `p21`, `p22` (Pāli verse lines) | `p.verse`, no gap between lines |
| `p19` (Ānandajoti's verse translation) | `p.verse-translation` |
| `p8` (a verse quoted in the story) | `p.verse-quote` |
| `p13` (hanging-indent lines) | `p.list-line` |
| `p14`, `p15`, `p23`, `p24` (indented) | `p.indented` |
| `p16` (stanza spacer, a no-break space) | `p.spacer` |
| a paragraph with a `t15` span (the overview, identification, Compare and Keywords lines) | also `p.translator-note` |
| italic `t11 t34 t39 t41 t42 t43 t62 t63 t65`; bold `t14 t33 t47`; `t60`; superscript `t45 t58 t64 t66` | `<i>`, `<b>`, `<b><i>`, `<sup>` |
| page-marker spans `t18 t35 t46` | `span.pts-ref` |
| size / colour-only spans | their text |

Markup not in these tables is listed in the report as unmapped and rendered as
its text.

**Page markers** (`split_page_marker()`): `[1.1]`, `{1.95}`, both in one span
(`[1.1] {1.95}`), a marker split over two spans, and a span holding only a
space. No new case was needed beyond the Dhammapada rules.

**Links** to another file or Jātaka become plain text: no `href` on a page
leads off it. A test checks that every `href="#…"` has its `id`.

## 6. The defect report

**Source defects are reported, never repaired**, and the text is kept
verbatim. `extract-report.txt` lists, each line naming the Jātaka and the file:

- a number missing from 1–547 (as a range list), appearing twice (the second is
  not written), or out of order;
- a Jātaka outside 1–547, before any Book heading (not written), or with no
  body text;
- a Book heading out of order, or a Book whose stated range ("464-473") does
  not match the Jātakas it holds;
- a heading that does not match `toc.ncx`, and a `toc.ncx` Jātaka that is not
  in the content files;
- a note ref with no note, a note never referenced, and a note whose number
  differs from its ref's;
- a verse number repeated within a Jātaka or section;
- a heading that is not "Ja <number> <name>".

For the 2023 epub: 547 Jātakas, 99 sections, 6096 verses, 1582 note references
to 1582 notes, 0 unmapped markup, and **32 defects — all of them a repeated
verse number**: Ja 35 (v. 1), 220 (vv. 1, 2), 392 (v. 5), 536 (vv. 1–11 and 13,
across the parts of the Great Section), 541 (v. 167), 546 (vv. 287, 288, 291,
294). There is no missing, repeated or reordered Jātaka, no `toc.ncx`
mismatch and no note defect. The list is in the committed
`extract-report.txt`.

The Nipāta of each page is compared with the `group_path` of its
`ja<N>.att/en/opentipitaka` row **after** the bootstrap, since the extractor
needs no database; for the shipped database all 547 agree.

## 7. Tests

- `jataka_commentary_anandajoti::tests`: a fixture epub folder
  (`bootstrap_cli/tests/data/jataka-commentary-anandajoti/Fixture-Jataka.epub/`,
  with `expected/` beside it) compared byte for byte, run twice, plus the link
  checks, a Jātaka crossing a file boundary, the sub-story list, the book's
  note numbers, the skipped front matter, and a planted defect in the report.
  `JA_COMM_BLESS=1` rewrites `expected/`. A real-data test runs on the epub
  when it is present: 547 pages, numbers 1–547 unique, every `#` link
  resolvable, a stable manifest.
- `import.rs` tests: the copied columns, the skip when the CST row is missing,
  the plain text, the real manifest's row count.
- `backend/tests/test_related_sutta_uids.rs`: the `ja62` tabs, pure and against
  the shipped database, and the m | a | t targets; `query_task::commentary_filter_tests`
  and the `searcher.rs` filter tests: the commentary rule in each search path;
  `src-ts/column_bar.test.ts`: the bottom-bar label.

## 8. The shared module, and a third Ānandajoti epub

`bootstrap_cli/src/bootstrap/anandajoti_epub/` holds what the Ānandajoti
imports have in common:

| File | What |
|---|---|
| `epub.rs` | `EpubFiles` (`from_zip()`, `from_dir()`, `spine()`, `toc()`), `TocEntry`, `parse_ncx()`, `sha256_hex()` |
| `source.rs` | the block stream of a content file, note files and note parsing, `text_without_refs()`, `is_page_marker()` |
| `page.rs` | `escape()`, `split_page_marker()`, `Writer` (inline text, note refs and the Notes section, anchors) driven by a per-book `ClassMap`, and `frame()` |
| `report.rs` | `Report`: defect lines, the summary line, stdout and `extract-report.txt` |
| `import.rs` | `CopiedColumns`, `page_plain_text()`, `check_epub_sha256()` |

What stays per book: the structure — vaggas and stories, Nipātas and
Jātakas — and its class tables, which is the whole of what a book's classes
mean. A further Ānandajoti epub gets its own
`bootstrap_cli/src/bootstrap/<book>_anandajoti/` module with a `ClassMap`
(`page_marker_classes`, `span_style()`, `paragraph_class()`), its structure
walk over the shared block stream, an importer that calls the shared helpers,
a `manifest.json`, a fixture folder with `expected/`, and a `…-extract`
subcommand added to `main.rs` with no `init_app_data()`. Then add the folder to
`bootstrap_sources()` (required) and a doc beside this one.

## 9. Reach

The pages exist only in a freshly bootstrapped `appdata.sqlite3`: users get
them with the next appdata release, not through an in-app update. A version
bump is the maintainer's, through `scripts/set-version.py`
([versions.md](./versions.md)).
