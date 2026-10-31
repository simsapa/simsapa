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

## 2. Goals

1. Import segmented Pāli for all CST records in place of today's HTML.
2. Create one `en/opentipitaka` record for every CST Pāli record.
3. Lose no text and no searchable text compared with today.
4. State in the footer of the text that this English is AI-translated, with its source and
   licence.
5. Never align two texts whose segment keys do not refer to the same passage.
6. Show a translation beside a Pāli text in the multi-column layouts, so that no reader
   concludes the text has none.
7. Stop a text that exists in both Pāli editions from appearing twice in search results,
   while keeping every text findable.
8. Remove the duplicate text stored by the large FTS tables, without slowing search, on a
   slow phone as well as on a desktop.

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
- As a user searching in Pāli, I want to choose which edition I see of a text that is in
  both, and I want the texts that are in only one edition to appear whatever I choose.
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
   must be reviewed and made consistent with requirement 1. `load_coverage` treats an
   absent database as "all unmapped" today, which must now be an error too, unless the
   whole CST import is being skipped.

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
    cost (requirement 44).

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
27. Put the footer markup in one function, so requirement 33 can reuse it.
28. `bridges/assets/qml/DhammaTextSourcesDialog.qml` must list
    `https://www.opentipitaka.org/` with a line saying the English is AI-translated.

### Display: which texts may be aligned

29. **Keys are only comparable within one text family, and two families can hold the same
    key for different passages.** `mn1:1.1` in `mn1/pli/cst` is not the passage `mn1:1.1`
    in `mn1/pli/ms` or `mn1/en/sujato`. `bilara_multi_column_html` matches columns on the key
    alone, so without a guard, these columns would show different passages side by side,
    with no error, and blank cells where only one side has the key.
30. Add one function, for example `columns_can_align(uids) -> bool`, returning true only if:
    - no uid is in the CST family, which is today's case; or
    - all uids are in the CST family, and all have the same base.

    The CST family is a uid ending in `/pli/cst` or `/en/opentipitaka`.
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
    `mn1.att/en/opentipitaka`, and the Tantivy filter requires `source_uid` `cst`, so with
    "include commentary" off, English commentary would still appear. Extend both to
    `opentipitaka`, and check the matching filter in `related_sutta_uids`, which goes by
    reference kind and should already work.
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

### Display: the default English column

45. When a Pāli text is opened with no `columns` requested, the default column set must be
    the opened text and an English, in every layout. Solo still renders the opened text
    alone, and keeps the English as page state, so that switching to Lines or Columns
    shows it. This mirrors opening a translation, which already pairs it with a Pāli.
46. The English is chosen in `resolve_sutta_display_options`:
    - for a `/pli/cst` text, `<base>/en/opentipitaka`;
    - for any other Pāli text, the first segmented English of the same base, then the first
      non-segmented one, in `sort_suttas` order, leaving out `opentipitaka`. A non-segmented
      English is dropped in Lines by the existing rule, and shown as a block in Columns.
    - If there is none, the Pāli text is shown alone.
47. Requested `columns` always win. Removing the English column must keep it removed while
    the page is open, and the API's `columns` parameter must behave as today.
48. The column options must always include the same-base partner of a CST-family text,
    whatever `include_cst_mula_in_translations` says. Otherwise, with the default settings,
    a reader who removes `mn1/pli/cst` beside `mn1/en/opentipitaka` cannot get it back.
49. The tooltip of a disabled option in Lines must not say "No segmented text", which is
    false for a CST text. It must cover both cases: not segmented, or from a different
    text family.

### Search: Pāli edition preference

50. `include_cst_mula_in_search_results` must default to true, so that CST-only texts are
    found. Existing users must get this too, through the `AppSettings` deserialize hook,
    because a stored `false` is the old default and not a choice. A choice made after the
    migration is kept.
51. Add the setting `pali_edition_preference`, with the values `ms`, `cst` and `both`. The
    default is `ms`, which keeps today's results for texts in both editions.
52. A record is **in both editions** when it is an MS Pāli record or a mapped CST mūla record
    and `sutta_refs_are_related` holds between it and a record of the other edition. So
    `an5.303/pli/ms` and `an5.303-1152/pli/cst` both count. CST `.xml` records and
    commentaries never count, because they have no comparable reference.
53. Preference `ms` hides the CST records that are in both editions, and preference `cst`
    hides the MS records that are in both. `both` hides nothing. A record in only one edition
    is never hidden, so Vinaya divisions stay findable under `ms`, and the Theragāthā
    records stay findable under `cst`.
54. The preference applies only when both editions are included. The existing "MS Mūla in
    Search" and "CST Mūla in Search" checkboxes keep their meaning, and exclude a whole
    edition.
55. The records in both editions must be computed once, at CST import, and stored in a new
    appdata table, `pali_edition_twins`, holding their uids. A table keeps `Sutta` unchanged,
    so a failed migration, which is non-fatal, cannot break every sutta read. The SQL
    filters read it, and the Tantivy indexer writes it into a new `has_other_edition` field,
    so there is one definition.
56. The filter must apply in ContainsMatch, FulltextMatch and every query that goes through
    `filter_suttas_query`, as the existing edition filters do.
57. The search API and the CLI keep `both`, so their results do not depend on a GUI setting.
58. The advanced search options must offer the preference beside the two checkboxes, enabled
    only when both are checked.

### Uid resolution with the new English records

59. `get_full_sutta_uid` must try `<code>/pli/cst` before its `LIKE`. The `LIKE` reads the uid
    index, where `/en/opentipitaka` sorts before `/pli/cst`, so a bare `mn1.att` would resolve
    to the AI English.
60. `get_sutta_by_range` must prefer a mūla record among those matching the suffix, because
    `.att` sorts before `/`. When none matches the suffix, it must prefer a Pāli mūla record
    over `results.first()`, which would otherwise be an AI commentary.

### Search: external-content FTS tables

61. These FTS tables must become external-content tables, which store no copy of the text,
    keeping their columns, `tokenize='trigram'` and `detail='none'`:

    | Table | Content table | Saves today |
    |---|---|---|
    | `suttas_fts` | `suttas` | 191 MiB, about 335 MiB with the English |
    | `dict_words_fts` | `dict_words` | 166 MiB |
    | `bold_definitions_fts` | `bold_definitions` | 115 MiB |
    | `bold_definitions_bold_fts` | `bold_definitions` | 14 MiB |

    `book_spine_items_fts` (15 MiB) and `dpd_headwords_fts` (2 MiB) stay as they are. Each
    table is decided on its own by requirement 64. This adds no database file and no
    migration: the tables are created by the `scripts/` SQL and ship with the new database
    version. The query code does not change, so the app works with either form of a table.
62. The delete and update triggers must use FTS5's `'delete'` command with the old values,
    under the same condition as the table's insert trigger, because a row that condition
    excludes was never indexed. The update trigger must be `AFTER UPDATE OF` the indexed
    columns only. `UNINDEXED` columns are now read from the content table, so renaming a
    dictionary, which updates `dict_label`, no longer re-tokenizes its rows.
63. An index must stay exactly in step with its content table, because FTS5 reports no error
    when it does not, and only returns wrong results. So the bootstrap must run
    `integrity-check` with rank 1 on each of these tables after the last import, and
    `import-cst` on `suttas_fts`, and an error must stop them.
64. **A table may change only if measurements show no slowdown**, on a desktop and on a slow
    phone. This is the gate for each table on its own:
    - `EXPLAIN QUERY PLAN` shows the virtual table's index in use. The FTS5 documentation does
      not say how an indexed `LIKE` behaves on an external-content table.
    - Every benchmark case returns the same result count as before.
    - For every case, cold and warm, the median after the change is at most the larger of
      110% of the median before and the median before + 20 ms.
    - Deleting a dictionary of about 2,000 entries meets the same limit, because `'delete'`
      has to read and tokenize the old text.

    A table that fails keeps its stored copy, and the others go ahead. The phone must be the
    slowest real arm64 device available, with internal storage, and on a card too if one is
    available. An emulator does not count, because it runs on the desktop's CPU and disk.
65. `Total rows in suttas_fts` in `cli_support.rs` must count `suttas_fts_docsize`. A
    `SELECT COUNT(*)` on an external-content table is answered from the content table, so
    it would include rows that are not indexed. Any other count of a changed table must do
    the same.
66. The benchmark must run the app's own query code on the app's own SQLite, so that it cannot
    measure a different statement or a different SQLite from the one users have. The
    `sqlite3` CLI is not a substitute: it is a different build.

## 5. Non-Goals (Out of Scope)

- Any change in `tipitaka-xml-parser`, or to the export format.
- Aligning CST texts with SuttaCentral texts. That needs a key mapping that does not exist.
- Sentence-level segmentation. The English aligns with the Pāli per row only.
- Correcting the English, including the wrong paragraph numbers in it.
- Any new search filter or author setting for `opentipitaka`.
- A separate optional download for this English.
- External-content tables for `book_spine_items_fts` and `dpd_headwords_fts`. They would
  save 15 MiB and 2 MiB, and the first reads through `content_html` to reach its text.
- Making the English findable through the file-name search.

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
  160 MB to `suttas_fts`, and the Pāli about 10 MB, so appdata would reach about 1.35 GB
  with the FTS table as it is. The external-content table of requirement 61 removes
  `suttas_fts_content`, 191 MiB today and about 335 MiB with the English, so appdata reaches
  about 1.0 GB. The English index gains about 160 MB.
- Requirement 61 also takes about 166 MiB from `dictionaries.sqlite3` (1,334 MiB, 12%) and
  129 MiB from `dpd.sqlite3` (2,443 MiB, 5%). These are uncompressed sizes.
- Where the text sits in the content table affects requirement 64. A search may read the row
  of each candidate, and SQLite reads a row's columns in order. In `suttas` and `dict_words`
  the text comes before the HTML. In `bold_definitions`, `commentary_plain` and `bold_ascii`
  come after `commentary`, so that table is the likeliest to fail.
- `create_fts5_indexes()` runs on the empty database, before the imports, so `suttas_fts` is
  filled by its insert trigger in a full bootstrap, and a full bootstrap needs no `rebuild`.
  `create_dictionaries_fts5_indexes()` runs before `import_migrate_dpd`, whose updates to
  `dict_words` then go through the triggers. The `bold_definitions` script runs after that
  table is filled.
- The runtime writes to these content tables all go through the triggers: language downloads
  (`asset_helpers.rs`), and dictionary import, rename and delete. `bold_definitions` is
  written only at bootstrap.
- The `pali_edition_twins` table needs one new appdata migration. The other columns and
  tables already exist. The new Tantivy field needs an `INDEX_VERSION` change.
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
4. Search finds, in a built database, with the edition preference on `both`: a phrase from
   each of a mūla, an `.att`, a `.tik` and an unmapped division, in Pāli and English; a
   variant reading; and the Dhammapada summary of requirement 17.
5. With "include commentary" off, no `.att` or `.tik` record is returned, in either language.
6. Opening `mn1/pli/cst` beside `mn1/en/sujato` never aligns them.
7. The AI footer and licence are visible in Solo, Columns and Lines, whichever text is
   opened first, when the English is shown.
8. `cargo test`, the TypeScript tests and a full bootstrap pass.
9. Requirement 44's measurements are recorded, and the maintainer accepts them.
10. Opening `mn1/pli/cst` or `mn1/pli/ms` in Lines or Columns shows an English with it.
11. With the default settings, a phrase in `mn1` returns the `pli/ms` record and not the
    `pli/cst` one, and a phrase in a Vinaya division returns its `pli/cst` record.
12. An existing user's stored `include_cst_mula_in_search_results: false` becomes true once.
13. For each table that passed requirement 64, its `_content` table no longer exists and
    `integrity-check` with rank 1 passes. The before and after figures, with the phone's
    model and Android version, are recorded for every table, including any that failed.

## 9. Open Questions

41. **Requirement 3.** Should an absent `fragments.sqlite3` during a partial or development
    bootstrap also be an error, or is there a mode that skips CST altogether?
42. **Header `<h3>` for English records.** It reuses the Pāli `{nikaya} {number}`. Confirm
    this reads well above an English title.
43. **`has_content_json` naming.** Once it means "can be aligned with the opened text",
    renaming it may read better, but it touches the TypeScript side.
44. **Size.** Before release, measure the appdata database, the English and Pāli fulltext
    indexes, and the compressed release download, against the previous build. The
    maintainer decides whether that is acceptable. Shipping in the main appdata is decided,
    but the increase has not been measured. The external-content tables of requirement 61
    are already part of this feature. If the size is still too large, `content_plain` and
    `content_json` hold the English twice. The templates are only about 4% of the growth.
