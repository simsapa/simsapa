# PRD: Ānandajoti's Jātaka Translation as Sutta Pages (`ja<N>.att/en/anandajoti`)

## 1. Introduction / Overview

`library-imports.toml` already imports Ven. Ānandajoti's *The Birth Stories
(Jātakatthavaṇṇanā) (Revised 2023)* (`The-Jataka.epub`, uid `jataka-comm`) as a
library book. The book is read in the library reader only. A user who opens a
Jātaka commentary page, e.g. `ja62.att/pli/cst`, does not see it among the
translations.

This feature breaks the epub into one sutta page per Jātaka at bootstrap time,
stored as `ja<N>.att/en/anandajoti`, so that:

- the translation tabs of `ja62.att/pli/cst` (and of the other `ja62.att/*`
  records) list it;
- the reader panel's bottom bar shows it as **"anandajoti (en, comm)"**.

It follows the finished Dhammapada work
(`tasks/2026-11-12-134735-prd---dhammapada-commentary-anandajoti.md` and its
tasks file, in `tasks/` or `tasks/archive/`;
`docs/dhammapada-commentary-anandajoti.md`).
Read that doc first: the uid scheme, the commentary rule, extract → import, the
anchor scheme, the class mapping and the defect-report rule all carry over.

**Goal:** every one of the 547 Jātakas in the epub is a sutta page that appears
as a translation of the matching CST commentary record, with its footnotes,
verse and sub-story anchors and PTS page markers intact.

## 2. Goals

1. 547 pages `ja1.att/en/anandajoti` … `ja547.att/en/anandajoti` exist in
   `appdata.sqlite3` after a bootstrap.
2. Each page opens in the reader and appears in the translation tabs of the
   matching `ja<N>.att/pli/cst` and `ja<N>.att/en/opentipitaka`, and of the
   Pāli mūla `ja<N>/pli/ms` when commentary-in-translations is on.
3. The bottom bar shows "anandajoti (en, comm)".
4. The pages are hidden from Contains/Fulltext/other searches when commentary
   search is off, and found when it is on.
5. The extraction is deterministic and reports source defects without
   repairing them.
6. The shared epub parts of the Dhammapada extractor are reused, not copied.

## 3. User Stories

- As a reader of `ja62.att/pli/cst`, I want a tab "anandajoti (en)" so that I
  can read Ānandajoti's translation beside the Pāli commentary.
- As a reader on a Jātaka page, I want the footnote pop-ups of the translation to
  work as they do in the library book.
- As a reader with commentary search switched off, I do not want Jātaka
  commentary translations in my sutta search results.
- As the maintainer, I want to re-run an extraction when a new epub revision is
  dropped in, and be told when the extracted pages are stale.
- As the maintainer, I want epub defects (a missing or duplicated Jātaka, a
  dangling note) reported verbatim to send to the author, never silently fixed.

## 4. Functional Requirements

### A. Extraction (epub → pages)

1. A bootstrap CLI subcommand `jataka-commentary-extract [--epub <path>]
   [--output <dir>]` reads `library-imports/books/The-Jataka.epub` (default from
   `source_paths.rs`) and writes, to `jataka-commentary-anandajoti/`:
   one `ja<N>.html` per Jātaka, `manifest.json`, `extract-report.txt`. It needs
   no appdata DB, and exits non-zero only when it cannot read the epub or write
   the output, never because of a source defect.
2. The epub is read as one stream of block elements in `content.opf` spine
   order, switching Jātaka on each heading, never cutting by file (a Jātaka may
   cross a file boundary). Headings are cross-checked against `toc.ncx`
   (entries of the form "Ja 62 Aṇḍabhūtajātaka The Birth Story about … (1s)"); a
   mismatch is reported.
3. The uid number `N` is the Jātaka number from the heading. The Pāli title
   (`Aṇḍabhūtajātaka`) is the page's `title_pali`; the English title is its
   `title`. The epub's TOC entries carry a trailing section label, "(1s)" or
   "(Mahānipāta)", which is the Nipāta, not part of the title: it is read into
   the manifest's `nipata` data and left out of `title`.
4. **Out of scope content stays out of the pages:** the front matter
   (Introduction, Studies, Bibliography, Abbreviations, "The Dates of the Jātaka
   Book", etc.) and the Nidānakathā (Distant, Intermediate and Recent Epochs)
   are not imported as pages. They remain readable in the library book
   `jataka-comm`. The Book / Nipāta headings ("Book I. Ekanipāta …") are not
   pages either; a page may carry the Nipāta name in its header.
5. The three notes files (`notes.html`, `notes2.html`, `notes3.html`) are read
   to resolve every note reference. A page's Notes section holds the notes
   referenced by that page, in order of first reference, **keeping the book's
   numbers** (as in the Dhammapada pages). A note ref with no note is reported.
6. Note refs use the app's `sup.footnote-reference > a` markup so that the
   footnote pop-up and bottom bar work; the note back-link `↩` is the existing
   `a.footnote-backref`, which the pop-up already strips.
7. PTS page markers (`{…}`, `[189]`, …) become
   `<span class="pts-ref">…</span>` using the existing `split_page_marker()`
   rule (marker split over two spans, a dash left outside the span). A marker
   shaped differently here is reported, not guessed.
8. Links to another file or Jātaka are written as plain text; no `href` leads
   outside the page. Every `href="#…"` has a matching `id` on the page.
9. Anchors: Jātaka section `ja-<N>`; sub-stories (Story of the Present, Story of
   the Past, verses, "Word-commentary", etc., as the epub's headings divide
   them) get `ja-<N>-<slug>` ids, de-duplicated within the page; one
   `<a id="ja-<N>-v<k>"></a>` per verse where the source numbers verses; notes
   `note-ref-<M>` / `note-<M>` as in the Dhammapada pages. The final anchor
   scheme is fixed in the first task after reading the real epub.
10. Presentation classes map to semantic ones as in the Dhammapada pages (verse
    lines keep their line breaks; the translator's additions stay italic;
    commentator's notes keep their own class). The survey of every span class,
    paragraph class, link, image and table in the epub is done first and
    recorded in the module doc, and anything unmapped is counted in the report.
11. The page frame is the whole `content_html`:
    `<div class="suttacentral html-text"><article id='ja62.att' lang='en'><header>…</header>…</article></div>`,
    with a header naming the Nipāta and the Jātaka's title, so the reader's
    sutta CSS applies and `sutta_html_to_plain_text()` keeps the title. The
    model is the header of `dhp21-32.att/en/anandajoti` (built in
    `dhammapada_commentary_anandajoti/page.rs`). The CST and OpenTipitaka
    `ja<N>.att` records have `content_html` NULL (they are segmented,
    `content_json` only), so there is no existing Jātaka page to copy.
    The `<div>` inside the article is `anandajoti-jataka-comm`; the Dhammapada
    pages keep `anandajoti-dhp-comm`, unchanged.
12. A page-top table of contents is built only if it helps the longer Jātakas
    (a sub-story list when there are two or more sub-stories), as in the
    Dhammapada pages; its link text excludes note refs and page markers.
13. `manifest.json` is pretty-printed with stable key order: `epub_file`,
    `epub_sha256`, and for each Jātaka `uid`, `file`, `jataka_number`, `title`,
    `title_pali`, `nipata` (name and number), `verse_count` (where applicable).
14. **Defect checks (reported, never repaired, each naming Jātaka and file):**
    a number missing from 1–547; a number appearing twice; Jātakas out of
    order; a count per Nipāta that differs from the epub's own overview, if it
    gives one; a note ref with no note and a note never referenced; a heading
    that does not match `toc.ncx`; a Jātaka with no body text. The report ends
    with a summary line (Jātakas, notes, defects) and is written to
    `extract-report.txt` and stdout.
14a. The extractor needs no DB, so it cannot compare a Jātaka's Nipāta with the
    `group_path` of its `ja<N>.att/en/opentipitaka` row (`Jātaka-aṭṭhakathā /
    1. Ekakanipāto / …`). That comparison is made at verification, after the
    bootstrap, and any difference is listed, never fixed.
15. **Determinism:** no timestamps, sorted maps or book-order vectors, stable
    anchor ids; two runs give identical bytes.

### B. Bootstrap import

16. A `JatakaCommentaryAnandajotiImporter` (the Dhammapada importer's
    counterpart) inserts one row per manifest entry:
    - `uid` `ja<N>.att/en/anandajoti`, `language` `en`, `source_uid`
      `anandajoti`, `source_language` `pli`;
    - `sutta_ref`, `nikaya`, `group_path`, `group_index`, `order_index`,
      `sutta_range_*` copied from the `ja<N>.att/en/opentipitaka` row (if it is
      missing, derive them as the CST import does and log a warning);
    - `title` and `title_pali` from the manifest; `content_html` the page file;
      `content_json` NULL;
    - `content_plain` from `sutta_html_to_plain_text()` with the Notes section,
      note-ref numbers and the page / sub-story `<nav>` lists removed;
    - `copyright` "Ānandajoti Bhikkhu"; `license` "CC BY-SA 4.0";
      `source_info` naming *The Birth Stories (Jātakatthavaṇṇanā) (Revised
      2023)*, Ānandajoti Bhikkhu, and
      **`https://ancient-buddhist-texts.net/English-Texts/Jataka/`**.
17. The importer runs after the CST import and the Dhammapada commentary
    importers, before the navigation trees and the fulltext index builds.
18. Preflight (`bootstrap_sources()`): the folder's `manifest.json` is a
    required source, with its path written once in `source_paths.rs`.
19. If `manifest.epub_sha256` differs from the SHA-256 of the epub in
    `library-imports/books/`, or the epub is missing, the bootstrap logs a
    warning "re-run jataka-commentary-extract" and continues.
20. The `jataka-comm` entry in `library-imports.toml` and `library_imports.rs`
    stay untouched; the book is still in the library.

### C. Translation tabs, bottom bar, search

21. The tab matching already treats `ja<N>.att/en/anandajoti` as a commentary
    translation of `ja<N>.att/pli/cst` (the rule is "reference ends in `.att` or
    `.tik`"). This is verified, with a pure test using the uid set of one Jātaka
    (`ja62/pli/ms`, `ja62/en/sujato`, `ja62.att/pli/cst`,
    `ja62.att/en/opentipitaka`, `ja62.att/en/anandajoti`, plus a neighbour
    `ja63.att/…` that must not match). A fix goes in `backend/src/helpers.rs`,
    not in the uids.
22. The reader panel's bottom bar shows "anandajoti (en, comm)" for it. The
    Dhammapada work already produces this label for `.att/en/anandajoti`; this
    is verified for a Jātaka uid, and a test is added only if the label code
    does not already cover it. `has_content_json` for the page is false.
23. Search: the commentary exclusion (any `.att` / `.tik` uid, whatever the
    source) already hides the pages when commentary search is off in all three
    paths (Diesel, FTS5, Tantivy). It is verified against the 547 new rows. The
    pages are indexed for Contains and Fulltext, and the Tantivy index counts
    547 new `en` docs.
24. The nikāya filter treats the pages as the CST rows do (the `nikaya` column
    is copied). The doc states this.

### D. Docs

25. `docs/jataka-commentary-anandajoti.md` (new) describes the current
    behaviour: epub facts, extract → import, when to re-run (SHA warning), uid
    scheme, anchors, class mapping, the defect rule, what is deliberately left
    out (front matter, Nidānakathā), and that verses not numbered in the source
    have no verse anchor. Add it to `docs/README.md` and to "Feature docs" in
    `CLAUDE.md`. Update `PROJECT_MAP.md` (module, subcommand, source folder).
    `docs/dhammapada-commentary-anandajoti.md` gets a cross-reference to the
    shared module. Docs say no history ("describe current behaviour").

## 5. Non-Goals (Out of Scope)

- The Nidānakathā, the Introduction, Studies, Bibliography, Abbreviations and
  the Rhys Davids essay as pages. They stay in the library book.
- Changing the Pāli, Sujato, Chalmers, Rouse, Francis or OpenTipitaka Jātaka
  records, or the SC spine.
- New UI: no new tab design, dialog or setting. The existing commentary
  settings govern the pages.
- Splitting long Jātakas (Mahā-ummagga, Vessantara, …) across several pages.
- Editing the epub: defects are reported, never repaired.
- Sub-story-level navigation-tree nodes.
- Removing the `jataka-comm` library book.
- Any Jātaka-specific "Present/Past story" structure beyond the epub's own
  headings and anchors.

## 6. Design Considerations

- Same visual language as the Dhammapada pages: in
  `assets/sass/_anandajoti.sass` the rules are scoped to `.anandajoti-dhp-comm`
  (the only place that name appears besides `page.rs`). Extend that selector to
  `.anandajoti-dhp-comm, .anandajoti-jataka-comm`, and add the Jātaka-specific
  rules. The Dhammapada pages' markup is not touched, so their expected files
  and stored `content_html` do not change. One set of theme-neutral rules
  covers light and dark; run `make sass`.
- Pages are long for some Jātakas; the page TOC is compact and lighter than the
  text. Check the largest page's size and load time on Android.

## 7. Technical Considerations

- **Reuse, not copy:** move the generic parts of
  `bootstrap_cli/src/bootstrap/dhammapada_commentary_anandajoti/` (zip / folder
  `EpubFiles`, OPF spine, NCX reading, note-ref markup, `split_page_marker()`,
  anchor slugging / de-duplication, SHA-256, the page-frame builder) into a
  shared module (e.g. `anandajoti_epub/`), and keep the Dhammapada structure
  logic where it is. The Dhammapada fixture tests (byte-for-byte expected
  files) must pass unchanged after the refactor; do the refactor first, in its
  own step, before any Jātaka code.
- `CopiedColumns` (in the Dhammapada `import.rs`) is also used by
  `dhammapada_tipitaka.rs` (the Daw Mya Tin import). It is shared code too: move
  it with the rest, update that import, and keep
  `cargo test dhammapada_tipitaka` green.
- **Size and cost (measured):** the epub's content files total 6.7 MB of HTML
  for 547 Jātakas (the Dhammapada epub's largest page is 329 KB). The largest
  existing `ja<N>.att` Pāli-English record (`ja542`) has 393 KB of `content_json`,
  so Mahā-ummagga, Vessantara and the other large Mahānipāta pages may reach
  ~1 MB with markup. Indexing 547 more `en` rows is small beside the existing
  3,000+ `ja` rows. `related_sutta_uids_like_pattern()` for a `ja` uid is `ja%`,
  so each tab lookup reads the `ja` uids (all ~3,000 of them, now ~3,550): fine,
  but the test run in 6.0 checks that the tabs open without a visible delay.
- **Reach:** the pages exist only in a freshly bootstrapped `appdata.sqlite3`.
  Users get them with the next appdata release (the version bump is the
  maintainer's, never done here), not by an in-app update.
- The epub differs from the Dhammapada one (3 notes files, ~140 content files,
  heading levels, class names): the survey comes first and is recorded in the
  module doc; do not assume the Dhammapada class map applies.
- Use `scraper`, never regexes over markup (except the one DOTALL strip for
  `content_plain`, as the Dhammapada importer does).
- `bootstrap_cli` already has `zip` and `scraper`; no new crate.
- Order: uid match with the CST rows requires the CST Jātaka records to exist
  (they do: 547 `ja*.att/pli/cst` and `ja*.att/en/opentipitaka`).
- Tests run against the dist `appdata.sqlite3`; any test that inserts rows uses
  `dummy-…` uids and deletes exactly those.
- A full re-bootstrap is needed to see the pages. Follow the global heavy-job
  rules: language server check, `free -h`, `df -h .` in the dist folder, a
  `systemd-run` memory-capped scope with limited parallelism, and tell the user
  the cap first.
- Do not commit `*.sqlite3`; ask before committing.
- GUI checks are the user's to do by hand.

## 8. Success Metrics

- 547 `ja<N>.att/en/anandajoti` rows after bootstrap; `ja<N>.att/en/opentipitaka`
  still 547 and `ja<N>.att/pli/cst` still 547.
- Opening `ja62.att/pli/cst` lists `ja62.att/en/anandajoti` in the tabs (both
  commentary settings, as for `.att/en/opentipitaka`), and "anandajoti (en,
  comm)" in the bottom bar.
- With commentary search off, a Contains and a Fulltext search for a word from
  one story finds none of the new pages; with it on, it finds them.
- Every `href="#…"` on every page has an `id`; no `href` leaves the page.
- `extract-report.txt` is committed with the extraction; its defects are listed
  in the commit message.
- Extraction run twice is byte-identical.
- The Dhammapada fixture tests and `cargo test` in `bootstrap_cli`, `backend`,
  `api`, `backend_cli` stay green (apart from known timed-test drift).
- Search pins (`search_results_pin.tsv`) regenerated and reviewed.

## 9. Open Questions (answered by the survey, tasks 3.0)

1. **Heading and class map:** Jātaka = `h3` "Ja N Pāli<br>English (label)"; Nipāta = the `h2` "Book <Roman>." heading; sections = `h4` (only 10 Jātakas have them). Present / Past stories and the word-commentary have no headings. The TOC's nested entries are the `h4` sections.
2. **Verse numbering:** numbered per Jātaka ("N. " before the Pāli verse); repeated and missing numbers exist and are reported.
3. **Jātakas with multiple CST uids?** No: 547 on both sides, contiguous.
4. **Largest page size:** about 295 KB of epub markup (Ja 546): no split.
5. **Bottom-bar label:** confirmed by a test (`ja62.att/en/anandajoti` → "anandajoti (en, comm)").
