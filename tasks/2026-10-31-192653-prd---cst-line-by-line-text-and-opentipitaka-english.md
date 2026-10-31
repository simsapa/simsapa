# PRD: Line-by-line CST text and OpenTipitaka English

## 1. Introduction / Overview

Simsapa shows CST Pāli as one HTML block per text. It cannot show that text line by line,
and it has no English for most of the commentaries, sub-commentaries and other CST texts.

`tipitaka-xml-parser` now exports that data. Its `import-opentipitaka` command adds two
tables to the fragments database: OpenTipitaka (OT) Pāli and English, row by row, for all
217 CST files. See `tipitaka-xml-parser/docs/opentipitaka-export.md`.

This feature imports those tables at bootstrap, so that:

- every CST text in Simsapa is segmented (Bilara `content_json`), and
- every CST text has an English translation under a new author, `opentipitaka`, which can
  be read line by line under the Pāli and found by search.

All of this English is AI translation, so the footer of each text must say so.

**Goal:** a user can open any CST text, read it line by line in Pāli and English, and find
it by searching in English.

### What the fragments database provides

| Table | Rows | Holds |
|---|---|---|
| `cst_texts` | 7,076, one per mapped sutta fragment, in 52 files | `code`, `pali_content_json`, `pali_content_json_tmpl`, `pali_variant_json`, `en_title`, `en_content_json`, `en_content_json_tmpl` |
| `cst_segments` | 421,922, one per row, in all 217 files | `cst_file`, `ot_id`, `frag_idx_code`, `rend`, `paranum`, `pali_html`, `pali_wrapper`, `variant`, `english` |

Segment keys look like `mn1:12.3` and `mn1.att:12.3`. Pāli and English of one text have
identical keys in identical order. Key order is reading order, so **never re-sort them**.

`tipitaka_xml_parser` (already linked by `bootstrap_cli`) exports:

- `opentipitaka::build_bilara_text(code, rows)`, which builds the same JSON from a slice of
  `SegmentRow`, and `impl From<&CstSegmentRecord> for SegmentRow`
- `opentipitaka::cst_text_code(cst_file, sc_code)`, which gives `mn1.att` from
  `s0201a.att.xml` and `mn1`
- `OT_AUTHOR_UID`, `OT_ENGLISH_FOOTER`, `OT_LICENSE`, `OT_SOURCE_URL`
- `fragments_models::{CstSegmentRecord, CstTextRecord}`, and the two tables in
  `fragments_schema`

### Input data

The bootstrap copy, `bootstrap-assets-resources/tipitaka-xml-data/fragments.sqlite3`, has
these tables filled (import report `reports/2026-10-31-203806-opentipitaka-import.md`).

### Related PRDs

- [Search: Pāli edition preference](./2026-10-31-215150-prd---search-pali-edition-preference.md) and
  [FTS: external-content space saving](./2026-10-31-215151-prd---fts-external-content-space-saving.md) were split off from this PRD.
  They ship with the same new database version, and this PRD does not depend on either.

## 2. Goals

1. Import segmented Pāli for all CST records in place of today's HTML.
2. Create one `en/opentipitaka` record for every CST Pāli record.
3. Lose no text and no searchable text compared with today.
4. State in the footer of the text that this English is AI-translated, with its source and
   licence.
5. Never align two texts whose segment keys do not refer to the same passage.
6. Show a translation beside a Pāli text in the multi-column layouts, so that no reader
   concludes the text has none.

## 3. User Stories

- As a reader of `mn1`, I want the CST Pāli with its English, line by line.
- As a reader of a commentary such as `mn1.att`, I want an English line under each Pāli
  line, because no other English exists for it.
- As a reader of a text with no SuttaCentral uid, such as a Vinaya commentary division, I
  want the same line-by-line view.
- As a user searching in English, I want to find passages that never had a translation.
- As a user searching in Pāli, I want to keep finding variant readings, as I do today.
- As a user comparing translations, I want human translations listed first, and I want to
  know that an English text was produced by AI.
- As a reader opening a Pāli text in Lines or Columns, I want an English beside it without
  having to look for one.
- As the maintainer, I want the bootstrap to stop if the data is missing, not to build a
  release without it.

## 4. Functional Requirements

### Bootstrap: input check

1. Before importing anything, the CST import must check the fragments database. It must
   stop with an error if `cst_segments` or `cst_texts` is missing, or if any source file
   has no rows in `cst_segments`. The message must say to run
   `import-opentipitaka --fragments-db <path>` in `tipitaka-xml-parser`.
2. There must be **no fallback** to importing Pāli from the XML. One code path is kept on
   purpose: a fallback could silently build a release with no English.
3. The behaviour when `fragments.sqlite3` is absent, or the `romn/` directory is absent,
   must be an error too, always. `load_coverage` treats an absent database as "all unmapped"
   today, and that must change. `--skip-appdata` already skips the whole CST import, so no
   new skip mode is needed.

### Records from mapped fragments

4. `import_from_fragments` in `bootstrap_cli/src/bootstrap/tipitaka_xml.rs` must read each
   record's JSON from `cst_texts`, by `(cst_file, frag_idx_code)`, instead of calling
   `xml_to_html`. A `Sutta` fragment that has an `sc_code` but no `cst_texts` row must be an
   error for the import. A fragment with `frag_review = 'moved'` is skipped, as today.
5. Fragments must be read `ORDER BY start_line, start_char`.
6. `VERIFIED_CST_FILES` and the coverage classes stay as they are. They decide which files
   take this path. A file off the list ignores `cst_texts` and goes through the division
   path (requirement 8).
7. `fragment_uid` must take its code from `cst_text_code`, so the two rules cannot drift.
   The navigation spine keeps calling `fragment_uid`.

### Records from divisions

8. `import_divisions` must build records from `cst_segments` with `build_bilara_text`, not
   from the XML. The toc still supplies uids and titles, and the XML is still read for
   `detect_nikaya_structure`. Add a function in `cst_coverage.rs` that cuts a file's rows
   into one slice per toc leaf, using the same rule as `split_into_divisions`:
   - A row opens a division when `pali_wrapper` contains `class='chapter'`. Do not use the
     `rend` column, which is NULL on such rows in `e0606n` and `e0607n`.
   - **Exception:** a chapter row with `ot_id` 0 is not a cut. `e0606n.nrf.xml` and
     `e0607n.nrf.xml` have an extra title there that repeats the marker.
   - Rows before the first marker are the head. If the toc has as many leaves as markers,
     prepend the head to the first division. If it has one more, the head is a division of
     its own. Any other count is a `SplitError`, as today, and the file is imported whole.

   `split_into_divisions` then has no caller, because only `import_divisions` calls it. It
   is removed with its helpers, and its tests move to the row-cutting function.
9. The whole-file fallback (`import_whole_file`) must use all of the file's rows the same
   way. `abh05t.nrf.xml` takes this path today.
10. The `code` passed to `build_bilara_text` is the record's uid base without `.xml`:
    `vin02m4.mul0` for the division `vin02m4.mul0.xml`, and `abh05t.nrf` for a whole file.

### Pāli records

11. Each Pāli record must store the JSON in `content_json` and `content_json_tmpl`.
    `content_html` must be `None`: `render_sutta_standard_body` prefers `content_json`, and
    this saves space. The uid stays `<code>/pli/cst`, and `source_uid` stays `cst`.
12. The Pāli text is now the OT text. It differs from the XML in one number
    (`s0404t.tik.xml`, row 693), and OT is the authority.
13. The exported template has no header. The import must add it to the **first key's**
    template, straight after the opening `<article id='…' class='cst'>`, keeping today's
    content: `<header><h3>{nikaya} {number}</h3><h1>{title}</h1></header>` for a fragment,
    and `<header><h1>{title}</h1></header>` for a division or a whole file. The title must
    be HTML-escaped, and any `{}` must not be introduced by it: the template's only `{}` is
    the segment value.
14. `consistent_niggahita` must be applied to the JSON, variants and titles, because
    Simsapa stores only `ṁ` and the OT text uses `ṃ`.
15. `pali_variant_json`, or for divisions the `variant` of each row, keyed by the segment
    key, must go into `sutta_variants` for the record, with `language` `pli` and
    `source_uid` `cst`. For division records, build it from `BilaraText.pali_variant_json`.
    Follow `import_sutta_variants` in `suttacentral.rs`. Variants must be inserted after
    the suttas, because they need `sutta_id`. Decided: the notes change from inline text to
    the ⧫ mark at the end of the row, hidden until opened unless
    `show_all_variant_readings` is on. This is the Bilara display, and the export has no
    position inside the row. The three rows of requirement 17 therefore show only the ⧫
    mark.
16. `content_plain` must be derived as Bilara texts do: render with `bilara_text_to_html`
    (no variants, comments or glosses), then `sutta_html_to_plain_text`. It must then have
    **the variant text appended**, each reading passed through `compact_plain_text` and
    joined by a space. Raw variant text would not match, because queries have their
    punctuation removed. `content_plain` has no newlines in any record, and must not gain
    them. Decided: search must keep finding what it finds today. Today notes are in the
    searchable HTML, and `sutta_variants` is not indexed.

    Derive `content_plain` from the template **before** the header of requirement 13 is
    added. `sutta_html_to_plain_text` keeps a header's `<h1>`, and CST `content_plain` has
    no title today.
17. This also covers the three rows whose Pāli is nothing but a note, so `pali_html` is
    empty: `s0502m.mul.xml` row 905 (the Dhammapada vagga summary), `s0510m1.mul.xml` row
    759 and `s0514m.mul.xml` row 2165. Their Pāli is only in `variant`, and a test must
    confirm it is searchable, with a phrase that crosses a comma. Their English is in the
    JSON as usual.

### English records

18. For every Pāli CST record, the import must create an English record with the uid
    `<base>/en/opentipitaka` (`OT_AUTHOR_UID`), where `<base>` is the Pāli uid's first path
    component: `mn1/en/opentipitaka`, `mn1.att/en/opentipitaka`,
    `vin02m4.mul0.xml/en/opentipitaka`.
19. It must have `language` `en`, `source_uid` `opentipitaka`, `source_language` `pli`, and
    the same `sutta_ref`, `nikaya`, `group_path`, `group_index`, `order_index` and range
    columns as its Pāli record. `content_html` is `None`.
20. `license` must be `OT_LICENSE`. The Pāli records must not get it: the OT licence applies
    to OT's own content, and the Pāli is credited to the Vipassana Research Institute.
21. Title:
    - fragments: `en_title`, or the Pāli title if it is empty (13 records in
      `s0305m.mul.xml`);
    - divisions: the English of the division's first chapter row, or the toc's Pāli title
      if it has none (a leading head division);
    - a whole file: the Pāli title, which is the file name stem;
    - `title_pali` is the Pāli title in both cases.
22. The header of requirement 13 must be added to the English template too, with the
    English title in the `<h1>`.
23. The import must apply `consistent_niggahita` to the English JSON and the English title,
    as `bilara_text_to_sutta` does for every language, because the English quotes Pāli
    terms. 65 `en_title` values contain `ṃ`.
24. `content_plain` is derived as in requirement 16, without variants, and before the header
    is added. The footer is removed by `sutta_html_to_plain_text`, so the AI notice is not
    indexed.
25. These records go in the **main appdata**. Decided: English already lives there, and
    this needs no new download path. The data ships with a new app version and a new
    database version, because `appdata.sqlite3` is kept across app updates. Measure the
    cost (see Size in section 9).

### Attribution

26. The exported English template ends with `OT_ENGLISH_FOOTER` in a
    `<footer class='noindex'>`. The import must add a second `<footer class='noindex'>`
    after it on the last key, reading:

    `Licence: CC-BY-NC-SA 4.0. Source: opentipitaka.org. Arranged in lines for Simsapa, with the niggahīta spelled ṁ.`

    The licence name is built from `OT_LICENSE` and links to the licence text
    (`https://creativecommons.org/licenses/by-nc-sa/4.0/`). The source links to
    `OT_SOURCE_URL`. CC BY-NC-SA 4.0 §3(a)(1) requires the link to the licence and a
    statement that the material was modified. This is required. The `license` column is
    not displayed anywhere in Simsapa today, so the column alone does not show it.
27. Put the footer markup in one function, so requirement 33 can reuse it. It cannot live in
    the parser: `backend` does not link `tipitaka_xml_parser`, and it should not, because
    only `bootstrap_cli` does. The reader needs the footer too, so the constants and markup
    go in `backend/src/opentipitaka.rs`, and a `bootstrap_cli` test asserts they equal the
    parser's.
28. `bridges/assets/qml/DhammaTextSourcesDialog.qml` must list
    `https://www.opentipitaka.org/` with a line saying the English is AI-translated.

### Display: which texts may be aligned

29. **Keys are only comparable within one text family, and two families can hold the same
    key for different passages.** `mn1:1.1` in `mn1/pli/cst` is not the passage `mn1:1.1`
    in `mn1/pli/ms` or `mn1/en/sujato`. `bilara_multi_column_html` matches columns on the key
    alone, so without a guard, these columns would show different passages side by side,
    with no error, and blank cells where only one side has the key.
30. Add one function, `columns_can_align(opened_uid, column_uids) -> bool`, returning true
    only if, counting the opened uid:
    - no uid is in the CST family, which is today's case; or
    - all uids are in the CST family, and all have the same base.

    The CST family is a uid ending in `/pli/cst` or `/en/opentipitaka`. The opened uid must
    count, because the aligned view uses the *opened* text's template
    (`app_data.rs:620`). The column bar can remove the opened text from the columns or
    replace it (`column_bar.ts:233`, `:295`), and `SuttaDisplayOptions::resolve` does not
    put it back. Opening `mn1/en/sujato` and showing `mn1/pli/cst` + `mn1/en/opentipitaka`
    would otherwise pass, and render the CST text on Sujato's template.
31. Every place that treats "has `content_json`" as "can be aligned" must also use this
    function, and non-alignable texts must then take the path that non-segmented texts take
    today:
    - `all_segmented` in `render_content_block_for_columns` (`backend/src/app_data.rs:596`):
      fall back to the unaligned block columns;
    - the Lines-mode drop in `backend/src/app_data.rs:431` and `:438`;
    - `has_content_json` in `TranslationData` (`backend/src/db/appdata.rs:454`), which the
      column bar reads to disable a text in Lines mode. It must be answered relative to the
      opened sutta, and the TypeScript tests updated.
32. `get_pali_for_translated` in `backend/src/app_data.rs` must return `<base>/pli/cst` for
    an `/en/opentipitaka` sutta, and `<base>/pli/ms` for every other author, as today. Its
    comment that `pli/cst` never has `content_json` must be corrected.
33. The aligned view uses the **opened** text's template, so the footer is missing when the
    Pāli is opened first. When any column is `/en/opentipitaka` and the opened text is not,
    the aligned view must add the footer from requirement 27 after the content. It must not
    show twice when the English is opened. A `<footer>` after `</article>` survives
    `html_get_sutta_page_body`, which only cuts documents containing `<html`.
34. `sort_suttas` in `backend/src/db/appdata.rs` must put `opentipitaka` after all other
    English translations.

### Search

35. `opentipitaka` English is indexed like any other English author, with no separate
    filter.
36. The **existing** CST commentary filter must also cover the English. `query_task.rs`
    tests `'%.att%/cst'` and `'%.tik%/cst'`, which do not match
    `mn1.att/en/opentipitaka`, and the Tantivy filter
    (`backend/src/search/searcher.rs:824`) requires `source_uid` `cst`, so with "include
    commentary" off, English commentary would still appear. Extend both to `opentipitaka`,
    and check the matching filter in `related_sutta_uids`, which goes by reference kind and
    should already work. `is_commentary` itself is already right for these uids, as
    `indexer.rs:106` reads it from the uid, so no index rebuild is needed for this fix.
37. The CST mūla filter stays Pāli-only. Decided: `mn1/en/opentipitaka` is an English
    translation of a mūla text, and is listed like other English.
38. The file-name search (`CstFileQuery`) keeps matching `%.xml/pli/cst`.

### Code that assumes `pli/cst` is not segmented

39. Review `backend/src/cst_files.rs`, `backend/src/query_task.rs` and
    `backend/src/sutta_json.rs`, which assume `.xml/pli/cst` and "pli/cst has no
    `content_json`", and update them and their comments. `sutta_json` will now return
    `Found` for CST texts.
40. Check that the code that falls back from `content_plain` to `content_html`
    (`query_task.rs:494`, `:518`, `:572`) still gets text, and that any other reader of
    `content_html` handles `None` on CST records.

41. `/sutta_bilara.json` must not return another family's variants. `sutta_json.rs:202` falls
    back to `<ref>/pli/ms` variants. For `mn1/en/opentipitaka` those belong to another text
    family, so it must read `<ref>/pli/cst`.
42. `translation_counts_conn` will count `/en/opentipitaka`, so nearly every navigation node
    gains one translation. This is correct for "translations installed", but it needs a test
    and a doc line.

### Re-running the import

43. `import-cst` must re-run cleanly. `main.rs` deletes only `%/pli/cst` rows before
    importing, and the importer skips a uid that exists, so the English rows must be cleared
    too, and the variants with them. The documented rebuild must also run
    `index rebuild --area suttas --lang en`, not only `--lang pli`.

### Display: the default English column

44. When a Pāli text is opened with no `columns` requested, the default column set must be
    the opened text and an English, in every layout. Solo still renders the opened text
    alone, and keeps the English as page state, so that switching to Lines or Columns
    shows it. This mirrors opening a translation, which already pairs it with a Pāli.
45. The English is chosen in `resolve_sutta_display_options`:
    - for a `/pli/cst` text, `<base>/en/opentipitaka`;
    - for any other Pāli text, the first segmented English of the same base, then the first
      non-segmented one, in `sort_suttas` order, leaving out `opentipitaka`. A non-segmented
      English is dropped in Lines by the existing rule, and shown as a block in Columns.
    - If there is none, the Pāli text is shown alone.
46. Requested `columns` always win. Removing the English column must keep it removed while
    the page is open, and the API's `columns` parameter must behave as today.
47. The column options must always include the same-base partner of a CST-family text,
    whatever `include_cst_mula_in_translations` says. Otherwise, with the default settings,
    a reader who removes `mn1/pli/cst` beside `mn1/en/opentipitaka` cannot get it back.
48. The tooltip of a disabled option in Lines must not say "No segmented text", which is
    false for a CST text. It must cover both cases: not segmented, or from a different
    text family.

### Uid resolution with the new English records

49. `get_full_sutta_uid` must try `<code>/pli/cst` before its `LIKE`. The `LIKE` reads the uid
    index, where `/en/opentipitaka` sorts before `/pli/cst`, so a bare `mn1.att` would resolve
    to the AI English.
50. `get_sutta_by_range` must prefer a mūla record among those matching the suffix, because
    `.att` sorts before `/`. When none matches the suffix, it must prefer a Pāli mūla record
    over `results.first()`, which would otherwise be an AI commentary.

## 5. Non-Goals (Out of Scope)

- Any change in `tipitaka-xml-parser`, or to the export format.
- Aligning CST texts with SuttaCentral texts. That needs a key mapping that does not exist.
- Sentence-level segmentation. The English aligns with the Pāli per row only.
- Correcting the English, including the wrong paragraph numbers in it.
- Any new search filter or author setting for `opentipitaka`.
- A separate optional download for this English.
- Making the English findable through the file-name search.
- The Pāli edition preference and the FTS space saving, which have their own PRDs.

## 6. Design Considerations

Opening `mn1/en/opentipitaka` in Lines mode shows it with `mn1/pli/cst`, aligned, with the
AI footer and licence at the end.

Opening `mn1/pli/cst` in Lines or Columns shows it aligned with `mn1/en/opentipitaka`, and
opening `mn1/pli/ms` shows it with `mn1/en/sujato`. In Solo they show the Pāli alone, and
switching layout brings in the English.

Opening `mn1/en/sujato` keeps today's default of `mn1/pli/ms`. If the user replaces that
column with `mn1/pli/cst` in Columns mode, it appears as an unaligned block. It cannot be
added beside `mn1/pli/ms`, because `arrange_repeat_pali` keeps one Pāli column. In Lines
mode it is disabled, like a non-segmented text.

In the Translation tab, the order for `mn1` is the Pāli texts, then human English, then
`opentipitaka`.

The exported wrappers use Simsapa's existing `.cst` styles, for example
`<p class='bodytext'>{}</p>` and `<p class='gathalast'>{}</p>`, inside
`<article class='cst'>`. A verse row is one segment, with its lines separated by `<br>`.

Add a `.cst .centre` rule to `suttas.css`, matching `.cst .centered`. 988 closing lines,
such as "Brahmajālasuttavaṇṇanā niṭṭhitā.", have that class, because `xml_to_html` maps it
for `<p>` but not for `<trailer>`, and today they are not centred.

`.cst .hangnum` has `margin-bottom: -18pt`, which draws the next row up beside a paragraph
number. In Lines this row is two lines tall, so the next row would cover the English number.
58,687 rows have this class. Set the margin to 0 for `.layout-lines`, and leave Solo and
Columns as they are.

## 7. Technical Considerations

- `SuttaRecord` in `tipitaka_xml.rs` needs `content_json`, `content_json_tmpl`, `license`,
  `source_language` and the variant JSON. `insert_suttas` hard-codes `content_json: None`,
  `license: None` and `source_language: Some("pli")`, and all of those must come from the
  record.
- Keep the fragments connection read-only, as `open_fragments_db` does.
- Read `cst_segments` per file, `ORDER BY ot_id`, so memory stays bounded.
- All 52 files with fragments are on `VERIFIED_CST_FILES` and fully mapped today, so the
  partially mapped path has no live data. It must still work.
- Division cuts on rows are safe. Measured across all 217 files: the rows and the XML have
  the same number of chapter markers once the two `ot_id` 0 rows are left out, every marker
  starts its row, and no row has two.
- `.att` and `.tik` sources have no notes, so their records have no variants. That is
  expected.
- Measured export size: the fragments database grows from 161 MB to 575 MB
  (`cst_segments` 385 MB, `cst_texts` 85 MB). This is bootstrap input and is not shipped.
- The dist appdata of 2026-10-30 is 864 MB, and the English and Pāli sutta indexes are 56 MB
  and 168 MB. Estimated from column totals, the English adds about 320 MB to `suttas` and
  160 MB to `suttas_fts`, and the Pāli about 10 MB, so appdata would reach about 1.35 GB. The
  English index gains about 160 MB. The FTS PRD could bring appdata down to about 1.0 GB.
- This feature needs no migration: the columns and tables already exist.
- `suttas.css` is generated from Sass. Change the source and rebuild it.
- Unit test the `columns_can_align` function and the row-cutting function with no database.
  Use a small fixture for the import.

## 8. Success Metrics

1. Every CST Pāli record has `content_json`, `content_json_tmpl` and no `content_html`.
2. Every CST Pāli record has exactly one `/en/opentipitaka` record with identical keys in
   identical order. Check without sorting the keys.
3. The set of Pāli CST uids differs from the previous bootstrap only by the fragment
   changes in the published database: `sn35.32-62.att` is replaced by `sn35.32.att`,
   `sn35.33-42.att`, `sn35.43-52.att` and `sn35.53-62.att`, and `sn24.19-96.att` and
   `sn24.19-96.tik` are added. `CstCoverageReport` differs by the same records.
4. Search finds, in a built database, with CST mūla included in the search: a phrase from
   each of a mūla, an `.att`, a `.tik` and an unmapped division, in Pāli and English; a
   variant reading; and the Dhammapada summary of requirement 17.
5. With "include commentary" off, no `.att` or `.tik` record is returned, in either language.
6. Opening `mn1/pli/cst` beside `mn1/en/sujato` never aligns them.
7. The AI footer and licence are visible in Solo, Columns and Lines, whichever text is
   opened first, when the English is shown.
8. `cargo test`, the TypeScript tests and a full bootstrap pass.
9. The size measurements of section 9 are recorded, and the maintainer accepts them.
10. Opening `mn1/pli/cst` or `mn1/pli/ms` in Lines or Columns shows an English with it.

## 9. Decisions

- **Absent fragments or `romn/`:** an error, always (requirement 3).
- **Header `<h3>` on English records:** keep it, as requirement 22 says. It reuses the Pāli
  `{nikaya} {number}`. Check by eye that it reads well above an English title (task 12.8).
- **`has_content_json` name:** keep it. Once it means "can be aligned with the opened text",
  renaming it may read better, but it is part of the documented `/translations_for_sutta`
  response, renaming touches the TypeScript side, and it brings no behaviour change.
- **Size:** still open. Before release, measure the appdata database, the English and Pāli
  fulltext indexes, and the compressed release download, against the previous build (tasks
  5.14 and 12.7). The measurement must report `suttas_fts` separately, because the English
  goes into its trigram index too. The maintainer decides whether that is acceptable.
  Shipping in the main appdata is decided, but the increase has not been measured. If it is
  too large, the FTS PRD is the first step. After that, `content_plain` and `content_json`
  hold the English twice. The templates are only about 4% of the growth.
