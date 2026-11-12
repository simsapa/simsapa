# PRD: Dhammapada CST uids in Simsapa, and the Ānandajoti Commentary as Sutta Pages

## 0. Prerequisite

This PRD depends on the parser PRD
`tipitaka-xml-parser/tasks/2026-11-12-142052-prd---dhammapada-sc-uid-mapping.md`.
That work splits the CST Dhammapada files (`s0502m.mul.xml`,
`s0502a.att.xml`) into one fragment per vagga, coded with the SuttaCentral
vagga uids (`dhp21-32`), and publishes the verified database to
`bootstrap-assets-resources/tipitaka-xml-data/fragments.sqlite3`.

**Do not start Part A until that database is published.** Check it first:
`xml_fragments` must have 26 Sutta fragments with an `sc_code` for each file,
and `cst_texts` must have their 52 rows (codes `dhp1-20` … and `dhp1-20.att`
…). The tests in A.3 and all of Part B can be written before then.

**Status: published on 2026-11-12.** The check above passed on the published
copy: 26 + 26 Sutta fragments with an `sc_code`, and 52 `cst_texts` rows. The
previous bootstrap DB is kept next to it as
`tipitaka-xml-data/fragments.2026-10-31-203806.sqlite3` for rollback. The
parser session report, with every fragment's line span, is
`tipitaka-xml-parser/reports/2026-11-12-153456-s0502-dhammapada.md`.

Each file also has two **Header** fragments, `0.0` (the XML prolog up to
`<body>`) and `27.0` (`</body>` to the end). Like every other file's, they hold
markup only and have no `sc_code`. A Simsapa check that "every fragment has an
`sc_code`" must skip them, as it does for the other verified files.

**Fragment fields, as published** (vagga 2 shown; the others follow the same
pattern):

| Field | `s0502m.mul.xml` | `s0502a.att.xml` |
|---|---|---|
| `sc_code` | `dhp21-32` | `dhp21-32` (the `.att` comes from `cst_text_code()`) |
| `cst_code` | `dhp.2` | `dhp.2` |
| `cst_sutta` | `2. Appamādavaggo` | `2. Appamādavaggo` |
| `sc_sutta` | `Appamādavagga` | `Appamādavagga` |
| `group_levels` | Nikaya `Khuddakanikāye`, Book `Dhammapadapāḷi` (id `kn2`), Vagga `2. Appamādavaggo` (id `kn2_2`) | Nikaya `Khuddakanikāye`, Book `Dhammapada-aṭṭhakathā`, Vagga `2. Appamādavaggo` (no ids) |
| `nikaya` | `khuddaka` | `khuddaka` |

The mūla and the aṭṭhakathā records of a vagga therefore get the **same
title** from `fragment_title()`. Only the record header (A.1.2) tells them
apart, which matters for the tab list (§6).

## 1. Introduction / Overview

**Part A: use the new CST Dhammapada uids in Simsapa.**
The CST edition has no Dhammapada ṭīkā; the two files are the mūla and the
aṭṭhakathā. Simsapa imports them today as toc divisions named after the file:
`s0502m.mul0.xml/pli/cst` … `s0502m.mul25.xml/pli/cst`, and
`s0502a.att0.xml/pli/cst` (Ganthārambhakathā) … `s0502a.att26.xml/pli/cst`,
each with an `/en/opentipitaka` record. Opening Dhammapada Aṭṭhakathā
chapter 2 in the Sutta Navigation opens `s0502a.att2.xml/pli/cst`, with
`s0502a.att2.xml/en/opentipitaka` in the tab bar. Neither is linked to
`dhp21-32`.

Once the parser's database is published and the two files are added to
`VERIFIED_CST_FILES`, the bootstrap imports them as mapped fragments:

| Records | Text |
|---|---|
| `dhp1-20/pli/cst` … `dhp383-423/pli/cst` (+ `/en/opentipitaka`) | the mūla vaggas |
| `dhp1-20.att/pli/cst` … `dhp383-423.att/pli/cst` (+ `/en/opentipitaka`) | the vagga commentaries |

The parser attaches every passage of the source to some uid, so the front and
back matter sit inside the vagga records:

- `dhp1-20` and `dhp1-20.att`: the opening "Namo tassa…", "Khuddakanikāye" and
  book heading. In the aṭṭhakathā also "(Paṭhamo bhāgo)" and the
  **Ganthārambhakathā** (prologue), with its own verses numbered 1–10.
- `dhp116-128.att`: the second-volume heading ("Namo tassa…", book heading,
  "(Dutiyo bhāgo)"), which opens vagga 9. "Aṭṭhamo vaggo." stays at the end of
  `dhp100-115.att`.
- `dhp383-423.att`: the **Nigamanakathā** (conclusion) and the closing
  colophon, through "Dhammapada-aṭṭhakathā sabbākārena niṭṭhitā.".
- `dhp383-423`: the mūla's closing lines after verse 423: the "(Ettāvatā …)"
  summary, the vagga and gāthā uddāna verses, and "Dhammapadapāḷi niṭṭhitā.".

The existing matching code then links them to the Dhammapada: the `.att`
suffix rule (`split_sutta_ref_kind()`), the verse-range overlap
(`related_sutta_uids()`), and the MS/CST twins (`pali_edition_twins`).

**Part B: import Ānandajoti's translation as sutta pages.**
Bhikkhu Ānandajoti's *Dhamma Verses Commentary: A Revised Translation of the
Dhammapada Aṭṭhakathā (Revised 2024)* is already in
`bootstrap-assets-resources/library-imports/library-imports.toml` (uid
`dhp-comm`, `Dhamma-Verses-Comm.epub`) as a library book. Part B cuts the epub
into one page per vagga, with all the stories of that vagga, and imports each
page as `dhp<start>-<end>.att/en/anandajoti`, the same reference as the CST
commentary record. With these uids:

- opening a Dhammapada vagga (`dhp21-32/pli/ms`, any edition) with **Include
  commentary in translations** on lists `dhp21-32.att/pli/cst`,
  `dhp21-32.att/en/opentipitaka` and `dhp21-32.att/en/anandajoti`;
- opening the CST commentary `dhp21-32.att/pli/cst` (e.g. from the Sutta
  Navigation) lists the Ānandajoti page beside the OpenTipitaka English;
- with **Include commentary in search results** on, all of them are found by
  sutta search; with it off, none are.

Part B has two stages: an **extract** command (run once, or when the epub is
updated) writes one HTML file per vagga plus a manifest, and an **import**
bootstrap step reads them.

**Goal:** a reader of a Dhammapada vagga can open its Pāli commentary and two
English translations of it in the same window, and can search them.

## 2. Goals

1. (A) After a bootstrap the database has `dhp*/pli/cst`, `dhp*.att/pli/cst`
   and their `/en/opentipitaka` records, 26 of each, and no `s0502m.mul*.xml`
   or `s0502a.att*.xml` records.
2. (A) The Sutta Navigation opens the Dhammapada Aṭṭhakathā vaggas under
   their new uids.
3. (B) All 26 vaggas have a page `dhp<start>-<end>.att/en/anandajoti` with
   every story of that vagga (305 in total), in book order and complete.
4. (B) Each page starts with a table of contents of its stories, and under
   each story heading there is a list of that story's sub-stories.
5. (B) Each page ends with the footnotes it refers to, with the book's note
   numbers and links in both directions.
6. (A+B) The commentary records are listed as translations of their vagga only
   while *Include commentary in translations* is on, and found by sutta search
   only while *Include commentary in search results* is on.
7. The `dhp-comm` library book keeps working unchanged.

## 3. User Stories

- As a reader of Dhammapada vagga 2, with commentary in translations on, I
  want the Pāli aṭṭhakathā, the OpenTipitaka English and Ānandajoti's
  translation in my translation tabs, so that I can read the stories behind
  the verses.
- As a reader browsing the Aṭṭhakathā in the Sutta Navigation, I want the
  Appamādavagga commentary to open with Ānandajoti's translation beside it.
- As a Pāli student reading `dhp1-20/pli/ms`, with CST mūla in translations
  on, I want the CST edition of the same vagga offered, so that I can compare
  the readings.
- As a reader, I want a table of contents at the top of the Ānandajoti page,
  so that I can jump to the story for verse 5.
- As a reader of a long story such as "2.1 The Story about Udena", I want its
  sub-stories listed under its heading, so that I can jump to "Part 5.
  Rejection of Māgandiyā by the Buddha".
- As a student, I want the translator's footnotes on the same page.
- As a researcher, with commentary in search on, I want a search for
  "Cakkhupāla" to find the Pāli commentary and both English translations.
- As a reader who has turned commentary off, I want none of these in my
  translation list or search results.

## 4. Functional Requirements

### Part A: CST Dhammapada uids in Simsapa

#### A.1 Import

1. Add `s0502m.mul.xml` and `s0502a.att.xml` to **both** verified lists, so
   that they stay in sync. This is the step that makes the bootstrap import
   their fragments instead of divisions (see
   `docs/sutta-navigation-tree.md` §3.1). The parser work publishes the
   database only and leaves both lists to this PRD.
   - `VERIFIED_CST_FILES` in `bootstrap_cli/src/bootstrap/cst_coverage.rs`:
     add the two names under a new `// Khuddaka` comment after the Aṅguttara
     group, and change the array length in the type (`[&str; 52]` →
     `[&str; 54]`).
   - `bootstrap-assets-resources/tipitaka-xml-data/verified_cst_files.txt`:
     first back it up next to itself, named after its modification time
     (`verified_cst_files.$(date -r verified_cst_files.txt +%Y-%m-%d-%H%M%S).txt`).
     Then add the two names as a new block: a blank line before it, one name
     per line, as the other nikāyas are laid out.
   - Check that the two lists name exactly the same files (diff the sorted
     names).
   - **Fix the test fixture that this breaks.** `tipitaka_xml.rs`'s test
     fixture uses `s0502m.mul.xml` as `DIVIDED_FILE`, "not on the list, with a
     head and two chapters, and a toc leaf for each", and asserts its
     `s0502m.mul0.xml/pli/cst` … uids (e.g.
     `every_path_stores_segmented_pali_and_no_html`). `cst_coverage::is_verified()`
     reads `VERIFIED_CST_FILES`, so once the name is on the list these tests
     fail. Move the fixture to another unverified file with the same shape
     (a head and two chapters, one toc leaf each, such as another Khuddaka
     `.mul.xml`). Update its rows and the expected uids, and keep what each
     test proves. Do not weaken the assertions to make them pass.
     `NOTE_ONLY_VARIANT` cites `s0502m.mul.xml` row 905, so check whether the
     moved fixture still has a note-only row, or keep that case on a fixture
     of its own.
2. After a bootstrap, the database must contain:
   - `dhp<range>/pli/cst` and `dhp<range>/en/opentipitaka`, 26 of each;
   - `dhp<range>.att/pli/cst` and `dhp<range>.att/en/opentipitaka`, 26 of
     each;
   - no `s0502m.mul*.xml/…` or `s0502a.att*.xml/…` records.

   Titles come from `fragment_title()` (`cst_sutta`, e.g. "2. Appamādavaggo").

   **Record header.** The `<h1>` header line that `fragment_header()` gets in
   `tipitaka_xml.rs` (built at about line 553 as `nikaya_and_number`) must read,
   by convention:
   - mūla `dhp21-32/pli/cst`: **"Dhammapada 21-32"**;
   - aṭṭhakathā `dhp21-32.att/pli/cst`: **"Dhammapada-aṭṭhakathā 21-32"**.

   As written, the code takes the nikāya name from the fragment's first
   `group_levels` entry, which is "Khuddakanikāye" for these files. It takes
   the number from `sc_code` with its leading letters stripped. The result
   would be "Khuddakanikāye 21-32", which does not name the book or say that
   21–32 are verses. Special-case the `dhp` prefix, and choose the
   aṭṭhakathā form from the source file (`.att.xml`, as `cst_text_code()` does).
   The Book level of `group_levels` does not give the word directly: it is
   "Dhammapada-aṭṭhakathā" in the aṭṭhakathā, but "Dhammapadapāḷi" in the
   mūla (see the field table in §0).
   Do not change the parser's `group_levels`: they stay
   `[Nikaya, Book, Vagga]`, as in the other nikāyas. The DN/MN/SN/AN headers
   must not change. Add a unit test covering one mūla vagga, one aṭṭhakathā
   vagga and one existing AN case.
3. `pali_edition_twins` must list `dhp<range>/pli/ms` and
   `dhp<range>/pli/cst` as twins. `store_pali_edition_twins()` computes this
   with no change; check the result.
4. Old `s0502*` uids are **not** carried over. No `sutta_uid_renames` rows,
   and no handling of old uids in bookmarks or history: the users are alpha
   testers and can start a new database.

#### A.2 Navigation tree and file-name search

5. Rebuild the navigation tree (the command is in
   `docs/sutta-navigation-tree.md`). The CST toc leaves keep their node ids
   (`cst:s0502a.att1.xml` … `cst:s0502a.att26.xml`, `cst:s0502m.mul1.xml` …)
   and now open the mapped uids: `cst:s0502a.att2.xml` opens
   `dhp21-32.att/pli/cst`. No `cst:frag:` child rows are made, because each
   leaf matches exactly one fragment of the same title ("2. Appamādavaggo"),
   and `build_fragment_branches()` collapses that case into the leaf. The
   aṭṭhakathā's `Ganthārambhakathā` leaf matches no fragment, so it opens the
   next record, `dhp1-20.att/pli/cst`, and is listed as a merged leaf (by
   design, not a defect). The SC spine's `dhp` leaves are unchanged.
6. A search for a CST file name (`s0502`) no longer finds these texts.
   `backend/src/cst_files.rs` documents this as expected for a mapped file.
   No change is needed; check that the query returns no results and no
   error.
7. Grep the repo for hardcoded `s0502` uids (tests, TOML, docs) and update
   them. `cst_files.rs` keeps the file names, because it lists source files,
   not uids.

#### A.3 Tests

8. A unit test (pure helper, no database) showing that for opened
   `dhp21-32/pli/ms`:
   - `related_sutta_uids(…, include_cst_commentary = true)` contains
     `dhp21-32.att/pli/cst`, `dhp21-32.att/en/opentipitaka` and
     `dhp21-32.att/en/anandajoti`, and not `dhp1-20.att/…`;
   - with `false` it contains none of the `.att` uids;
   - `related_sutta_uids_like_pattern("dhp21-32/pli/ms")` matches the `.att`
     uids;
   - for opened `dhp21-32.att/pli/cst`, the result contains
     `dhp21-32.att/en/anandajoti` and `dhp21-32.att/en/opentipitaka` whatever
     the commentary setting (same kind as the opened text).

   If any of these fail, fix the matching. Do not change the uids.
9. A search-filter test showing that `dhp21-32.att/pli/cst` and
   `dhp21-32.att/en/anandajoti` are excluded when
   `include_cst_commentary_in_search_results` is off and included when it is on,
   including the Tantivy path. Also show that `dhp21-32/pli/cst` follows the
   CST mūla setting.
10. Add `dhp21-32/pli/ms` / `dhp21-32/pli/cst` as a case in the existing
    `pali_edition_twins()` tests.

### Part B: Ānandajoti commentary pages

#### B.1 Source attribution

11. `bridges/assets/qml/DhammaTextSourcesDialog.qml` must list the source with
    the link `https://ancient-buddhist-texts.net/English-Texts/Dhamma-Verses-Comm/`.
    **This entry is already there (line 73).** Check that it is still there;
    see Open Question 2.

#### B.2 Extraction command (`bootstrap_cli`)

12. Add a `bootstrap_cli` subcommand, e.g.
    `dhammapada-commentary-extract --epub <path> --output <dir>`, that reads
    `Dhamma-Verses-Comm.epub` and writes the per-vagga files. Run it by hand
    once, and again only when the epub is updated.
13. The default output folder is
    `bootstrap-assets-resources/dhammapada-commentary-anandajoti/`. Add the
    folder name as a constant in `bootstrap_cli/src/bootstrap/source_paths.rs`,
    with a path helper like `munindo_bilara_dir()`.
14. Read the spine order from `Ops/content.opf`, and the chapter / story /
    sub-story structure from `Ops/toc.ncx`, checked against the HTML
    headings. What is known about the epub:
    - Vagga starts are `<h1>` headings, e.g. "1. The Chapter about the Pairs,
      Yamakavagga" (`007.html`). A vagga runs on across several files. The
      English and the Pāli name are split by a `<br/>`, with or without a
      space after the comma (`…the Pairs,<br/>Yamakavagga`). Split on the
      `<br/>`, never on the comma. `038.html` holds only an empty
      `<h1>&nbsp;</h1>`, which is not a vagga.
    - Stories are `<h2>` headings numbered `<vagga>.<n>`, e.g. "1.1 The Story
      about the Elder Cakkhupāla": 305 of them. Their class varies (`h20`,
      `h21`, `h22`), so match the number, not the class. The only other `<h2>`
      is the Prologue's. The `<h3 class="h30">` right after it is the Pāli
      story title ("Cakkhupālattheravatthu").
    - The story metadata comes next as `p7` paragraphs: a verse line
      (`Dhp 1`, `Dhp 54-55`), "Burlingame: …", "Compare: …", a summary,
      "Cast: …", "Keywords: …", and a `****` separator.
    - Sub-stories are `<h4 class="h40">` headings (185 of them), e.g. "1a Story
      of the Past: …" or "Part 5. Rejection of Māgandiyā by the Buddha". The
      three `<h5>` headings are in the Conclusion only.
    - **A file may begin in the middle of a story** (`015.html` begins at
      "15b The Novice Tissa", `037.html` at "33c. Story of the Present…").
      Assign content by heading order, never by file.
    - Footnote references are `<a href="notes.html#aNNNN"><span …>{N}</span></a>`
      with nine different span classes (`t8` 294, `th42` 28, `t15` 70, `t19`,
      `t24`, `t26`, `th12`, `th34`, `th44`). Some are inside headings (`th42`).
      Recognise a reference by its `href`, never by its class. The note texts
      are all in `Ops/notes.html`, one `<p class="p16">` each, opening with
      `<a id="aNNNN">` and a link back to the source file
      (`href="003.html#a12"`).
    - Page markers are `t10` spans, often both forms in one span
      (`<span class="t10"> {1.3} [28.146]</span>`), and they can sit inside a
      verse line.
    - Verse paragraphs (`p6`) break their lines with `<br/>`.
    - The epub has its own `<a id="aNN">` and `<a id="ncxNN">` anchors
      throughout. They are dropped from the output.
15. The Introduction (`003.html`), Abbreviations, Prologue (`006.html`,
    Ganthārambhakathā) and Conclusion (Nigamanakathā, `039.html`; `038.html`
    is empty) must
    **not** be written to any vagga page. They stay readable in the library
    book. The Pāli records differ here at both ends: the Pāli prologue is
    inside `dhp1-20.att/pli/cst`, and the Pāli Nigamanakathā and colophon are
    inside `dhp383-423.att/pli/cst`, because the parser attaches every passage
    to some uid (see §1). The English pages follow the book's own vagga split,
    so the first and last vagga pages are expected to be shorter than their
    Pāli records at those ends. That is not a defect to report.
16. Find each vagga's verse range from its stories' `Dhp …` lines, and check
    it against the 26 SuttaCentral vagga ranges (`dhp1-20`, `dhp21-32`, …,
    `dhp383-423`, the same list as the parser PRD's FR 4).
17. **Report source defects, never repair them.** Print a report, and still
    write the files, when:
    - a verse is covered by no story, or by two stories;
    - a story's verses fall outside its vagga's range;
    - a story has no `Dhp …` line;
    - a footnote reference has no note;
    - a vagga's story count differs from the book's Conclusion (`039.html`).
      The Conclusion gives each vagga's count in words, in two phrasings: "In
      the Chapter about the Pairs are fourteen stories." for the first, and "In
      the Chapter about Heedfulness there are nine." for the rest, some
      followed by a note ref (`{414}`). It also gives the total, 305.

    Text is kept verbatim, including spellings and numbering such as "33c.".
18. Output per vagga: `dhp<start>-<end>.html`, holding only the body fragment
    (no `<html>`/`<head>`, no epub CSS or fonts).
19. Output `manifest.json`: one entry per vagga, in order, with `uid`
    (`dhp1-20.att/en/anandajoti`), `file`, `vagga_number`, `title` ("1. The
    Chapter about the Pairs"), `title_pali` ("Yamakavagga"), `verse_start`,
    `verse_end`, `story_count`. It also records the epub's file name and
    SHA-256.
20. The output must be the same, byte for byte, every time the command runs
    on the same epub.

#### B.3 Page content

21. Each vagga page contains, in order:
    1. A table of contents with one entry per story, linking to the story,
       e.g. "Dhp 1 — 1.1 The Story about the Elder Cakkhupāla".
    2. The stories in book order. Each story has:
       - its heading (with its anchor), the Pāli title, and the metadata
         block;
       - **right after the heading block, a list of its sub-stories** (one
         level, in book order) linking to each sub-story heading. A story with
         no sub-stories has no such list;
       - the story text, with the verse paragraphs (`p6`) and the sub-story
         headings with their anchors.
    3. A "Notes" section with only the notes this page refers to, in order of
       first reference, keeping **the book's note numbers**.
22. Anchor ids, stable and unique within the page:
    - story `story-1-1`; sub-story `story-1-1-1a`, `story-2-1-part-5`, or
      `<story id>-sub-<n>` when a heading has no usable label;
    - before each story heading, one empty `<a id="dhp-v5"></a>` per verse the
      story covers (for a later jump-to-verse feature);
    - footnote reference `note-ref-4` and footnote `note-4`, with a back-link
      from each note to its first reference.
23. Links to other vagga pages or to files that are not imported become plain
    text. No link on the page may lead nowhere.
24. Page markers are kept as inline `<span class="pts-ref">{1.3}</span>`
    (and the `[28.146]` form), small and muted. Add the CSS in
    `assets/sass/` if no existing class fits.
25. Map the epub's presentation classes (`p3`, `p6`, `p7`, `t12`, `h21`, …) to
    a few semantic classes (`verse`, `story-meta`, `story-title-pali`,
    `substory-heading`) styled in the app's sutta CSS, in light and dark
    themes.

#### B.4 Bootstrap import step

26. Add a bootstrap importer, e.g.
    `bootstrap_cli/src/bootstrap/dhammapada_commentary_anandajoti.rs`, called
    from `bootstrap/mod.rs` with the other sutta importers, before the
    fulltext indexes are built.
27. For each manifest entry, insert one `suttas` row:
    - `uid` `dhp<start>-<end>.att/en/anandajoti`;
    - `sutta_ref`, grouping and range columns: as on the
      `dhp<start>-<end>.att/en/opentipitaka` row (or derived the same way);
    - `language` `en`, `source_uid` `anandajoti`, `source_language` `pli`;
    - `title` the English vagga title, `title_pali` the vagga name;
    - `content_html` the page file; `content_json` NULL;
    - `content_plain` derived from `content_html` with
      `sutta_html_to_plain_text()`, as `dhammatalks_org.rs` does, after
      removing the Notes section (note text is not in the plain text);
    - copyright / license / source_info naming the book, the translator and
      the URL.
28. Add the source folder to `bootstrap_sources()` in
    `bootstrap_cli/src/bootstrap/preflight.rs` as **required** (translations
    are always required), with the path written once in `source_paths.rs`.
    Run `cd bootstrap_cli && cargo test test_full_bootstrap_sources -- --ignored`.
29. The importer logs a warning (and does not fail) when the manifest's epub
    SHA-256 differs from the epub in `library-imports/books/`: "re-run
    dhammapada-commentary-extract".
30. The `dhp-comm` library import is unchanged.

#### B.5 Translations and search

31. Opening any `dhp<range>` vagga with *Include commentary in translations*
    on lists `dhp<range>.att/pli/cst`, `…/en/opentipitaka` and
    `…/en/anandajoti`. With it off, none of them are listed (tested in A.3).
32. Opening `dhp<range>.att/pli/cst` (e.g. from the navigation tree) lists the
    Ānandajoti page beside the OpenTipitaka English.
33. The Ānandajoti page has no `content_json`, so its entry is disabled in the
    Lines layout like other non-aligned texts (`has_content_json` false).
34. The pages follow the commentary search filter in every mode that applies
    source filters (tested in A.3).

### Documentation

35. Add `docs/dhammapada-commentary-anandajoti.md`, covering:
    - the epub facts in B.2;
    - the extract → import stages and when to re-run the extraction;
    - the uid scheme shared with the CST records;
    - the anchor scheme;
    - the defect report.

    Add a line for it in `CLAUDE.md` under "Feature docs" and an entry in
    `docs/README.md`.
36. Update `docs/sutta-navigation-tree.md` (the verified file count, now 54,
    and the Dhammapada Aṭṭhakathā as fragment rows), and `PROJECT_MAP.md`.

## 5. Non-Goals (Out of Scope)

- The parser work (the parser PRD in §0).
- Renaming or redirecting old `s0502*` uids in user data.
- Per-story (vatthu) uids. The unit is the SuttaCentral vagga uid.
- A Dhammapada ṭīkā: the CST edition has none.
- Importing the Introduction, Abbreviations, Prologue or Conclusion of the
  Ānandajoti book as sutta pages.
- Bilara segmentation of the Ānandajoti translation, or aligning it in the
  Lines layout.
- Navigating to a single verse's story from a link such as `dhp5`.
- Removing or changing the `dhp-comm` library book.
- Any change to the Daw Mya Tin (`/en/daw`) records.
- Correcting source text (CST, OpenTipitaka or the epub).

## 6. Design Considerations

- The Ānandajoti page uses the app's sutta CSS, with no epub fonts, in both
  themes.
- The tables of contents are compact lists, lighter than the story text, so
  a vagga with 14 stories does not push its first story far down.
- Verse paragraphs keep their line breaks and are set apart like verses in
  other translations; story metadata is visually secondary.
- The column-bar dropdown labels each text by author and edition only
  (`option_text()` in `src-ts/column_bar.ts`: "anandajoti (en)",
  "Pāli (cst)"), and it does not mark commentaries. The reader already knows
  whether the opened page is a commentary. So on `dhp21-32/pli/ms` the
  Ānandajoti verses and the Ānandajoti commentary both read "anandajoti (en)",
  as the OpenTipitaka mūla and commentary English already do. No change to
  the dropdown. The window's tabs show the full uid
  (`dhp21-32.att/en/anandajoti`).

## 7. Technical Considerations

- **Order of work:** the A.3 helper tests (no database needed), then Part B's
  extraction command, then, once the parser database is published (§0),
  A.1–A.2 and the Part B import step, then one re-bootstrap for both.
- Never commit `fragments.sqlite3` or `appdata.sqlite3`.
- Tests against the dist database must delete exactly the rows they insert
  (`dummy-…` prefix). Prefer pure helper tests.
- **The commentary search filter is CST-only today.** All three search paths
  exclude a commentary only when its source is `cst` or `opentipitaka`
  (`query_task.rs`, the Diesel filter and the FTS5 `extra_where`;
  `searcher.rs` `add_sutta_filters()`). FR 9 and FR 34 need it to exclude any
  `.att` / `.tik` uid, whatever the source. No uid in today's database
  contains `.att`/`.tik` without being CST or OpenTipitaka, so the change
  affects only the new rows.
- Extraction parsing: use the HTML/XML crates `bootstrap_cli` already uses
  (see `library_imports.rs`, `tipitaka_xml.rs`), not regexes over markup. Add
  a small fixture in `bootstrap_cli/tests/data/` (a story crossing a file
  boundary, one with sub-stories, two footnotes) with an expected-output test.
- The Tantivy `en`/`pli` indexes are built from the `suttas` table in the
  bootstrap, so no index code should change. Check that the new rows are
  counted after a bootstrap.
- The largest Ānandajoti page (vagga 1, ~340 KB of epub HTML before cleanup)
  is bigger than a typical sutta page. Check that it opens without a
  noticeable delay on Android.
- A re-bootstrap is required. No app-side migration is needed.

## 8. Success Metrics

- After a bootstrap: 26 each of `dhp*/pli/cst`, `dhp*/en/opentipitaka`,
  `dhp*.att/pli/cst`, `dhp*.att/en/opentipitaka`,
  `dhp*.att/en/anandajoti`; zero `s0502*` records.
- In the Sutta Navigation, Dhammapada Aṭṭhakathā vagga 2 opens
  `dhp21-32.att/pli/cst`, and its tabs offer `dhp21-32.att/en/opentipitaka`
  and `dhp21-32.att/en/anandajoti`.
- With commentary in translations on, `dhp21-32/pli/ms` lists all three
  `.att` records; with it off, none.
- With commentary in search on, Contains and Fulltext searches for
  "Cakkhupāla" find `dhp1-20.att/pli/cst` and `dhp1-20.att/en/anandajoti`;
  with it off, neither.
- The extraction report shows 305 stories, each assigned to one vagga. Any
  reported gaps, overlaps or count mismatches are listed in the commit message
  for the author, not repaired.
- Every `href="#…"` on every Ānandajoti page has a matching `id`
  (extraction test).

## 9. Open Questions

1. Should the Ānandajoti pages also have their own nodes in the navigation
   tree, or is reaching them through the tabs of `dhp*.att/pli/cst` enough?
   (This PRD assumes the tabs are enough.)
2. Is the existing "Dhamma Verses Commentary" line in
   `DhammaTextSourcesDialog.qml` acceptable as it is, or should it say that
   the stories are now shown beside the Dhammapada vaggas?
3. Should "Compare: Thag-a 95" in the story metadata become a link where the
   reference resolves? (This PRD assumes plain text.)
