# PRD: Segmented Munindo and Nyanadipa translations, and Dhammapada verse numbers

## 1. Introduction / Overview

The bootstrap imports two English translations as whole HTML pages:

- **Ajahn Munindo, *A Dhammapada for Contemplation*** — one record per Dhammapada
  chapter (`dhp179-196/en/munindo`), built by `DhammapadaMunindoImporter`
  (`bootstrap_cli/src/bootstrap/dhammapada_munindo.rs`) from
  `<bootstrap assets>/dhammapada-munindo/html/dhp-N.html`.
- **Bhante Nyanadipa, *The Silent Sages of Old*** — 36 Sutta Nipāta suttas
  (`snp4.2/en/nyanadipa`), built by `NyanadipaImporter`
  (`bootstrap_cli/src/bootstrap/nyanadipa.rs`) from
  `<bootstrap assets>/nyanadipa-translations/texts-sc-numbering/*.md`.

These records have `content_html` only and no `content_json`. Because of that,
they cannot be shown line by line beside the Pāli in the multi-column reader
(Lines / Columns layouts), and `/sutta.json` cannot serve them. Sujato's,
Ṭhānissaro's and Bodhi's translations can, because they are keyed by the
SuttaCentral segment ids of the Pāli `ms` text.

This feature has five parts:

1. **Segment the two translations.** Write Bilara-style segment files for every
   Munindo chapter and every Nyanadipa sutta, aligned to the Pāli `ms` segment
   ids, check that each file translates the right Pāli text, and make the
   bootstrap import these files instead of the HTML/Markdown pages.
2. **Show Dhammapada verse numbers at each verse group.** The Pāli heading of a
   verse group (the verse story title, e.g. *Ānandattherapañhavatthu*) shows the
   verse range in front of it: `183–185. Ānandattherapañhavatthu`. The same
   segment in the English translations (Sujato, Ṭhānissaro, Munindo) shows only
   the range, `183–185.`, so the number appears above that verse group in every
   column.
3. **Keep relevant introductions.** A chapter- or sutta-level introduction by the
   translator is kept, the way Ṭhānissaro's introductions are kept. Book-wide
   front matter is not imported into the sutta records.
4. **Show line breaks and indentation in verse.** The reader renders the line
   breaks a translation carries inside a verse segment (Ṭhānissaro, Bodhi,
   Munindo), and Ṭhānissaro's indentation, which suttas-core will carry as
   leading tabs (§6a, parts G and H).
5. **Keep Sujato as the default English** beside the Pāli, once Munindo and
   Nyanadipa are segmented (FR 15a).

**Goal:** Munindo and Nyanadipa read line by line beside the Pāli like the other
segmented translations, and a Dhammapada reader can always see which verses a
group is.

## 2. Goals

1. All 26 Munindo chapters and all 36 Nyanadipa suttas are imported with
   `content_json` and `content_json_tmpl`, and `content_html = NULL`.
2. Every segment file is checked against its Pāli text, and every mismatch found
   (wrong chapter or sutta, missing or extra verses, misplaced verses) is fixed
   and recorded in a review report.
3. Joining the segments of a file in order, ignoring whitespace and line breaks,
   gives back the translator's own text exactly, apart from the documented
   removals (verse numbers, front matter) and recorded corrections.
4. In the Dhammapada (all 26 chapters), every verse-group heading segment of the
   Pāli `ms` record starts with its verse range, and the same segment in the
   Sujato, Ṭhānissaro and Munindo records holds only that range.
5. The bootstrap fails, rather than silently falling back to HTML, if the
   segment files are missing.
6. Verse segments show their translator's line breaks in every layout, and
   Ṭhānissaro's verse shows its indentation once the regenerated suttas-core
   database is imported.
7. Sujato remains the default English beside the Pāli Dhammapada and Sutta
   Nipāta texts.
8. Every Munindo, Nyanadipa and Ṭhānissaro Dhammapada record shows its chapter
   or sutta title in the text in every layout (Solo, Lines, Columns), in the
   segment the Pāli template renders as the page title.

## 3. User Stories

- *As a reader of the Dhammapada*, I want Ajahn Munindo's translation in a
  column beside the Pāli, so I can see which Pāli lines each part of his
  rendering answers to.
- *As a reader*, I want to see `183–185.` above a verse group in every column,
  so I can cite a verse, compare it with a printed book, or find it again.
- *As a reader of the Sutta Nipāta*, I want Bhante Nyanadipa's verses lined up
  with the Pāli verses, as Sujato's are.
- *As a reader*, I want Nyanadipa's footnotes to show as footnotes (marker plus
  the note in the footnote bar), as Ṭhānissaro's do, and not as inline text.
- *As the maintainer*, I want a review report listing every place where a
  source file and the Pāli did not match, so I can check the decisions and
  report errors to the translators' publishers.

## 4. Functional Requirements

### A. Segment files: location and format

1. Segment files must be stored in a new `bilara/` folder inside each source
   folder:
   - `dhammapada-munindo/bilara/<sutta_id>.json`, one per chapter, named by the
     SuttaCentral range id (`dhp179-196.json`).
   - `nyanadipa-translations/bilara/<sutta_id>.json`, one per sutta
     (`snp4.2.json`).
2. Each file must use the **suttas-core segmentation form**, as documented in
   `suttas-core/docs/data-layers.md` ("`segmentations/<translator>/<sutta_id>.json`")
   and used by `suttas-core/data/segmentations/than/dhp179-196.json`:

   ```json
   {
     "sutta_id": "dhp179-196",
     "translator": "munindo",
     "input": "markdown/dhp-14.md",
     "input_hash": "<textHash of the source text>",
     "pali_hash": "<hash of the ordered Pāli segment-id list>",
     "segments": [
       {"id": "dhp179:0.1", "text": "", "source": "copy"},
       {"id": "dhp179:1",   "text": "The Buddha’s perfection is complete;\nthere is no more work to be done.\n", "source": "manual"}
     ]
   }
   ```

   - `segments` must list **every** segment id of the Pāli `ms` root file, in the
     Pāli file's order, including ids whose text is `""`.
   - `input` names the source file the segments were cut from, relative to the
     source folder. For Munindo this is `markdown/dhp-N.md`, not `html/`: the
     HTML files are only pandoc output of the Markdown (`md-to-html.sh`).
   - Hashes follow suttas-core's rules: `input_hash` is `textHash` of the source
     text (footnote markers removed, whitespace collapsed, so re-indenting a
     verse or moving a `[^n]` does not make the file stale), and `pali_hash`
     hashes the ordered Pāli segment-id list joined by `\n`. The check command
     (FR 14) uses them to report a stale file.
   - `source` uses suttas-core's values: `copy` for a segment taken over
     unchanged (empty or structural segments) and `manual` for text placed by
     hand. `manual` means "never overwrite without asking", as in suttas-core.
   - **Footnote markers are not in the segment text**, as in suttas-core.
     Footnotes are kept in a separate file (FR 10).
3. The importers must read only these files (and the footnote files, FR 10) for
   the text. The `html/`, `markdown/` and `texts-sc-numbering/` files stay in
   the source folders as the input the segment files were made from.

### B. Segment files: content rules

4. **Faithful to the author's text.** Joining a file's segment texts in order
   must reproduce the translator's text line for line, in the translator's line
   order. The only allowed differences are:
   - the translator's own verse numbers (`183.\\`, `186-187.\\`, `1\.`) are
     removed (they are replaced by the verse-group number, part C);
   - book-wide front matter is not included (FR 11);
   - Markdown is converted to the inline HTML the reader uses (italics `<i>` /
     `<em>`, curly quotes kept as they are);
   - corrections recorded in the review report (FR 13).
5. **Placing lines into segments.** The translator's lines must be distributed
   over the segments of the Pāli verse they translate, with `\n` marking a line
   break inside a segment, as the Ṭhānissaro data does
   (`"dhp187:3": "He is\none who delights\nin the ending of craving,\n"`).
   - When a translation verse has the same number of lines as the Pāli verse,
     place one line per segment.
   - When the counts differ, spread the lines so each segment holds the lines
     closest in meaning to its Pāli line, and keep the order of the
     translator's lines even when that differs from the Pāli order. A reader
     reading down the column must read the author's verse unchanged.
   - Every line must be in exactly one segment. No line may be dropped or
     repeated.
6. **A translation verse covering more than one Pāli verse** (Munindo's
   `186-187.` and similar): the lines must be split across those Pāli verses at
   the point where the translation passes from one Pāli verse to the next, as
   Sujato and Ṭhānissaro do for dhp186–187. Where the translation cannot be
   split at a verse boundary without breaking a sentence, the split must still
   keep the author's line order, and the case must be listed in the review
   report.
7. **Munindo's own verse numbers are not a reference.** His book sometimes
   numbers verses individually where the Pāli groups them, and sometimes groups
   verses differently from the Pāli verse stories. The segmenter must decide
   which Pāli verse each line translates **from the meaning**, comparing with
   the Pāli and with Sujato's and Ṭhānissaro's translations, and must not
   derive the alignment from Munindo's numbers. A place where Munindo's number
   disagrees with the meaning-based placement must be noted in the review
   report.
8. **Titles.** The translator's chapter or sutta title must be kept in the
   text, in the segment the Pāli template renders as the page title, following
   Sujato's layout:

   | Source | Segment | Template markup | Value |
   |---|---|---|---|
   | Munindo `dhp-14.md` | `dhp179:0.3` | `<h1 class='range-title'>` | `14. The Buddha` |
   | Nyanadipa `snp4.2.md` | `snp4.2:0.2` | `<h1 class='sutta-title'>` | `The Cave` |
   | Ṭhānissaro `dhp179-196/en/thanissaro` (FR 24a) | `dhp179:0.3` | `<h1 class='range-title'>` | `14. Awakened` |

   - The Dhammapada value is the chapter number and the translator's own
     title, as Sujato writes `14. The Buddhas`. The number tells a reader
     which chapter they are in, since these titles do not translate the vagga
     names word for word.
   - Nyanadipa's value is the **short heading of his Markdown file**
     (`The Cave`), not the longer form on the book's contents page
     (`The Octad on the Cave`). The heading must name the same sutta as the
     Pāli title of that SuttaCentral number (FR 12).
   - The division segments above the title (`dhp179:0.1`, `0.2`;
     `snp4.2:0.1`) stay `""`. In the Lines layout the Pāli column already shows
     *Khuddakanikāya* / *Dhammapada* / *Sutta Nipāta 4.2* there, and an empty
     cell adds no line.
   - Why this segment: in the multi-column views the record's `title` is not
     shown anywhere. The column header shows only the column label
     (`bilara_multi_column_html()` in `backend/src/helpers.rs`), so a title
     not in a segment is lost on screen. Four of Ṭhānissaro's Dhammapada
     chapters show what that looks like today: `dhp179-196` has
     `title = "Awakened"` while `dhp179:0.3` is `""`, so no title appears in
     his column. The other 22 carry his title in `0.3`. FR 24a gives all 26
     the same `<n>. <title>` form.
   - In the Solo layout the same segment is the page's `<h1>`, so the title
     appears exactly once in every layout.
   - The record's `title` field stays as it is today (`The Buddha`,
     `The Cave`), so search results and the navigation tree do not change.
   - The verse-group heading segments (`dhpN:0`, and `0.4` at a chapter start)
     must be `""` in the file; the importer fills them (part E).
9. **Prose segments.** Where the Pāli has prose segments (e.g. the narrative
   frame of `snp3.6` Sabhiya) and the translator has no translation of them,
   those segments must be `""`. Where the translator does translate them, they
   are placed like verse lines.
10. **Footnotes (Nyanadipa).** Nyanadipa's 58 `[^n]` notes are the book's
    endnotes (the *Notes* section, p. 175 of *The Silent Sages of Old*), so
    the Markdown files already hold all of them. Munindo's chapters have none.
    - In suttas-core, footnotes are not stored in segmentation files. They are
      derived at build time from the `[^marker]` positions in the raw text
      (`src/segment/footnotes.ts`), and `footnotes/<translator>.json` holds only
      overrides. Simsapa then reads them from the `segment_footnotes` table
      (`sutta_id, translator, language, segment_id, number, marker,
      word_index, content, editor`).
    - These sources have no suttas-core build to derive them, so the placement
      must be **stored**, in one file per source:
      `nyanadipa-translations/bilara/footnotes.json`, with the same fields as a
      `segment_footnotes` row:

      ```json
      { "footnotes": [
        {"sutta_id": "snp4.2", "segment_id": "snp4.2:3.2", "marker": "1",
         "number": 1, "word_index": 2,
         "content": "“Incorrigible” for <i>avadāniyā</i>. …", "editor": false}
      ] }
      ```

      - `number` is the 1-based reading order within the sutta; `word_index`
        is the 0-based word the note follows in the stored segment text, or
        `null` for the whole segment (suttas-core's meaning).
      - `content` is the note converted from Markdown to HTML, with the
        translator's wording unchanged.
      - The placement must follow suttas-core's rules (a marker glued to the
        previous word follows that word, etc.).
    - The importer must write them to `sutta_comments` in the `ssp-fn` stored
      form through `backend/src/footnotes.rs` (`encode_footnotes`), and must
      turn sutta references into links through `footnote_sutta_links.rs`, as
      `SuttasCoreImporter` does (`footnotes_value()` in
      `bootstrap_cli/src/bootstrap/suttas_core.rs`). Share that code; do not
      copy it.
11. **Introductions.** Neither source has an introduction for a single chapter
    or sutta, so no record gets a `description`:
    - Munindo's chapter files hold only the title and the verses.
    - Nyanadipa's book (`docs/The_Silent_Sages_of_Old22.*`) holds an editor's
      introduction, two prefaces, the suttas and the endnotes. Each sutta
      starts directly with its verses.
    - The book-wide pages (Munindo's `book-opening.md`, `preface-*.md`,
      `introduction.md`, `note-on-the-text.md`, `copyright.md.erb`; Nyanadipa's
      editor's introduction and prefaces) are not imported into any sutta
      record.
    - The importers must still support a `description` the same way the
      Ṭhānissaro import does (`suttas.description`, its text prepended to
      `content_plain`), as an optional field of the segment file
      (`"introduction": "<html>"`), so that one can be added later without
      code changes.

### C. Verification against the Pāli

12. Before a file is written, the segmenter must check that the source page is
    the translation of that Pāli text:
    - Munindo `dhp-N` ↔ the chapter range of `DHP_CHAPTERS_TO_RANGE[N]`: the
      chapter title matches the Pāli vagga, the first and last verses match the
      first and last Pāli verses of the range, and every Pāli verse of the
      range has a translation (or is noted as missing).
    - Nyanadipa `snpX.Y` ↔ `snpX.Y` in the Pāli `ms` text: the verse count
      matches the Pāli verses, and the file's short heading names the same
      sutta as the Pāli title of that number (`The Cave` ↔
      *Guhaṭṭhakasutta*, `Sabhiya` ↔ *Sabhiyasutta*). The book's contents
      page, which pairs each long English title with its Sn number, confirms
      the pairing. A file whose heading and verses belong to a different Pāli
      sutta than its file name says is a mismatch: its segment file is made
      for the correct SuttaCentral number, and the case is recorded (FR 13).
      The heading text itself is kept as written.
    - The book's contents page gives the Sutta Nipāta verse range for every
      sutta (`3. Sabhiya [Sn 3:6, 510-547]`), and it can be used as a second
      check of the verse count.
    - Four files have a different number of verses from the Pāli's numbered
      segment groups, and must be examined first: `snp1.12` (15 / 17),
      `snp3.6` (38 / 54), `snp4.16` (21 / 23), `snp5.18` (19 / 8). In at least
      `snp3.6` the difference is the Pāli prose frame, since the book's range
      510–547 is 38 verses. Each must be explained in the review report.
    - Nyanadipa's numbering differs from SuttaCentral's in the Pārāyanavagga
      (his `texts/snp5.1.md` is SC `snp5.2`; SC `snp5.1`, the introductory
      verses, is not translated). The import uses only `texts-sc-numbering/`;
      the check confirms each file's content matches its SC number.
13. Every mismatch found must be **fixed in the segment file** (correct Pāli
    target, correct placement) and **recorded** in a review report,
    `tasks/<date>-review---munindo-nyanadipa-segmentation.md`, one entry per
    case: the file, the segment ids, what the source has, what the Pāli has,
    and what was done. The source `html/` / `markdown/` files are never edited.
14. A check command must exist (in `bootstrap_cli`, e.g.
    `bootstrap_cli check-segment-files <source folder>`) that, for each segment
    file:
    - confirms the segment ids are exactly the Pāli `ms` ids in Pāli order;
    - confirms the joined text equals the source text after the documented
      removals (FR 4), and prints the first difference when it does not;
    - confirms `input_hash` and `pali_hash` still match the current files, and
      reports the file as stale when they do not;
    - confirms every footnote number of the source appears exactly once.
    It reports every problem and exits non-zero if there is any. The bootstrap
    does not run it; it is run after writing or editing segment files.

### D. Importing the segment files

15. `DhammapadaMunindoImporter` and `NyanadipaImporter` must build each record
    from its segment file:
    - `content_json`: one key per key of the template, in template order, the
      value from the file or `""` (same rule as suttas-core, using `IndexMap` +
      `serde_json::to_string`, never `serde_json::Value`).
    - `content_json_tmpl`: the template of Simsapa's own `<sutta_id>/pli/ms`
      record, read from the database. The SuttaCentral import runs before these
      importers, so the Pāli record must exist; if it does not, the import of
      that record must fail with an error naming the uid.
    - `content_plain`: through `bilara_html_and_plain_text()`, without
      footnotes, with the introduction prepended as for Ṭhānissaro.
    - `content_html = NULL`; `consistent_niggahita()` applied to every segment,
      note and introduction.
    - `description`: the introduction (FR 11), if any.
    - Footnotes written to `sutta_comments` in the `ssp-fn` stored form.
    - The uids stay the same as today (`dhp179-196/en/munindo`,
      `snp4.2/en/nyanadipa`), so bookmarks, links and the navigation tree keep
      working. `title`, `sutta_ref`, `nikaya`, `source_uid` and the range fields
      are derived as today.
    - `source_info`: `"<source folder>/bilara"` (`dhammapada-munindo/bilara`,
      `nyanadipa-translations/bilara`), so the record says where its text came
      from.
15a. **Sujato must stay the default English beside a Pāli text.**
     `default_english_for_pali()` (`backend/src/app_data.rs`) takes the first
     segmented English in uid order and moves only suttas-core records
     (`source_info` starting with `suttas-core`) to the back. `…/en/munindo` and
     `…/en/nyanadipa` sort before `…/en/sujato`, so once they are segmented they
     would become the default beside the Pāli Dhammapada and 36 Sutta Nipāta
     suttas. The rule must change to: first a segmented English imported from
     SuttaCentral (`source_info` is NULL), then any other segmented English,
     then any English, still leaving out the AI English. Tests:
     `dhp1-20/pli/ms` and `snp4.2/pli/ms` default to `…/en/sujato`; a Pāli text
     whose only segmented English is from suttas-core still gets it.
16. The HTML/Markdown import code paths for these two sources must be removed.
17. Both sources must become **required** in the bootstrap preflight
    (`bootstrap_cli/src/bootstrap/preflight.rs`; today both are `optional`),
    pointing at the `bilara/` folders, with the folder names as constants in
    `source_paths.rs`. The `if path.exists()` skip in `bootstrap()`
    (`bootstrap_cli/src/bootstrap/mod.rs`) must become an error. This follows
    the project rule that translation sources are never optional.

### E. Dhammapada verse-group numbers

18. A verse group is a run of verses starting at a Pāli heading segment and
    ending at the verse before the next heading segment, or at the last verse of
    the chapter. A heading segment is a `dhpN:0` segment, or the last `dhpN:0.x`
    segment of a chapter's first verse (`dhp179:0.4`), whose Pāli text is the
    verse story title.
    - Example: `dhp183:0` *Ānandattherapañhavatthu*, next heading `dhp186:0`
      → range `183–185`.
    - Example: `dhp179:0.4` *Māradhītaravatthu*, next heading `dhp181:0` →
      range `179–180`.
    - A group of one verse is written as a single number: `181`.
    - The range separator is an en dash (U+2013), never a hyphen: `183–185`.
19. The ranges must be computed from the **Pāli `ms` segment ids** by one
    shared function (e.g. in `bootstrap_cli/src/bootstrap/helpers.rs`), and
    never from any translator's numbering. The ids alone are enough: every
    template, root and translation file of a Dhammapada chapter has the same
    ids, so each importer can compute the ranges from the record it is
    building.
20. In the **Pāli** `dhp…/pli/ms` records, the SuttaCentral importer
    (`bootstrap_cli/src/bootstrap/suttacentral.rs`) must prefix each heading
    segment with the range: `"dhp183:0": "183–185. Ānandattherapañhavatthu "`.
21. In the **translation** records, the heading segment must hold only the range
    followed by a full stop, `"183–185."`, regardless of what it held before
    (it is empty in all three today, except Ṭhānissaro's `dhp239:0`, FR 22):
    - Sujato `dhp…/en/sujato` — in the SuttaCentral importer;
    - Ṭhānissaro `dhp…/en/thanissaro` — in `SuttasCoreImporter`
      (`bootstrap_cli/src/bootstrap/suttas_core.rs`);
    - Munindo `dhp…/en/munindo` — in `DhammapadaMunindoImporter`.
22. If a translation's heading segment is not empty before the range is
    written, the importer must log a warning naming the uid, segment id and
    text, and must still write the range (the text is reported, not merged).
    One case is known: Ṭhānissaro's `dhp239:0` holds `235-238*`, his web page's
    endnote citation for the *previous* verse group, carried into the next
    heading by suttas-core. It becomes `239.` with the warning, and the review
    report lists it as a suttas-core defect to report upstream.
23. The range must be written into `content_json` before `content_plain` is
    computed, so it is part of the plain text in the same way as the rest of
    the record.
24. No other Dhammapada translation (other languages in `sc-data`) and no Sutta
    Nipāta record gets verse numbers in this feature.

### E2. Ṭhānissaro's Dhammapada chapter titles

24a. `SuttasCoreImporter` must write `<chapter number>. <title>` into the
     chapter-title segment of each of Ṭhānissaro's 26 Dhammapada records
     (`dhp1:0.3` … `dhp383:0.3`, the segment the Pāli template wraps in
     `<h1 class='range-title'>`; find it by that markup), e.g. `14. Awakened`,
     `1. Pairs`. All 26 get the same form:
     - 22 chapters already carry his title in that segment (`Pairs`, `Fools`,
       some with trailing newlines, `"Happy\n\n"`). The title is that text,
       trimmed.
     - 4 chapters have it empty (`dhp21-32`, `dhp76-89`, `dhp179-196`,
       `dhp273-289`). The title is the record's `title` (suttas-core's
       `raw_translations.title`: `Heedfulness`, `The Wise`, `Awakened`,
       `The Path`).
     - If the segment's text and the record title differ, log both and use the
       segment's text.
     - If both are empty, log a warning and leave the segment empty.
24b. The chapter number comes from `DHP_CHAPTERS_TO_RANGE` by the record's
     range, never from parsing the title. The record's `title` field is not
     changed.

### F. Tests and documentation

25. Unit tests for the range function: chapter-start heading (`0.4`), a
    single-verse group, the last group of a chapter, all 26 chapters producing
    contiguous ranges that together cover 1–423 without gaps or overlaps.
26. Importer tests (against the real bootstrap assets, skipped when absent, as
    the current tests are): `dhp179-196/en/munindo` has `content_json` with the
    same keys as the Pāli template, `dhp183:0` = `"183–185."`, and the joined
    text contains *Refrain from doing evil*; `snp4.2/en/nyanadipa` has its
    footnote in `sutta_comments` and none of its text in `content_plain`.
27. A test that the Pāli `dhp183:0` content is `183–185. Ānandattherapañhavatthu`
    after the SuttaCentral import step.
27a. A test that `dhp179-196/en/thanissaro` has `dhp179:0.3` = `14. Awakened`
     and `dhp183:0` = `183–185.`, and that `dhp179-196/en/munindo` has
     `dhp179:0.3` = `14. The Buddha`.
28. The existing tests that compare against `tests/data/nyanadipa/snp4.10.html`
    and check Munindo HTML parsing must be replaced by tests of the new import.
29. Tests that write to the database must follow the project rule for the dist
    database: delete exactly the rows they inserted, with `dummy-` fixtures.
30. A new doc, `docs/munindo-nyanadipa-segmentation.md`, describing: the file
    format, the placement rules (FR 4–11), the check command, the verse-group
    rule, and where each importer fills the range. Add it to the notable-docs
    list in `CLAUDE.md` and to `PROJECT_MAP.md`.
31. Update `docs/suttas-core-import.md` where it mentions the Dhammapada /
    Nyanadipa / Munindo step and Nyanadipa's `sup.footnote-reference` markup,
    since Nyanadipa's footnotes now use the `ssp-fn` form. Its §3 (Ṭhānissaro)
    must describe the Dhammapada verse-group ranges and chapter titles the
    importer adds (FR 21, FR 24a).
32. Update the text-processing doc
    (`docs/text-processing-for-contains-match-and-fulltext-match-search.md`)
    where it mentions these two sources' HTML.
33. The doc lists the values of the `source` field (FR 2).
33a. The new doc also describes the verse-segment line contract (FR 35) and
     the renderer function; `docs/sutta-display-settings-and-multi-column-view.md`
     gets a short section pointing to it, since the rule applies in every
     layout.
33b. `docs/simsapa-cli-and-localhost-api.md` §13.4 / §13.5 state that a
     segment's text may contain `\n` (line breaks) and leading `\t` (indent
     levels). That doc is compiled into the CLI and must stay self-contained:
     no links to other docs or source paths.
33c. Tests for FR 34–40 in `backend/src/helpers.rs`: Ṭhānissaro's `dhp187:3`
     gives three line spans with indent levels from its tabs; a trailing `\n`
     adds nothing; a segment without line data is byte-identical to today; a
     footnote marker lands inside the right line; a prose segment is
     unchanged; the multi-column path gives the same line spans as the
     single-text path.

### G. Line breaks and indentation in verse segments (reader)

The decision and the examples behind it are in §6a.

34. The Rust renderer must show the line breaks and the indentation a
    translation carries inside a **verse segment**, a segment whose template
    entry contains `class='verse-line'`. It is done by one shared function in
    `backend/src/helpers.rs`, called from both the single-text path
    (`bilara_text_to_segments()`) and the multi-column path
    (`bilara_multi_column_html()`), so the Solo, Lines and Columns layouts show
    the same lines.
35. The segment-text contract (shared with suttas-core, FR 41):
    - `\n` separates lines. A `\n` at the end of the segment, with any
      whitespace after it, ends the last line and renders nothing.
    - A line may start with tab characters (`\t`); their number is the line's
      indent level, 0 being flush. A tab anywhere else is ordinary whitespace.
36. Rendering: each line becomes `<span class='ssp-line ssp-indent-N'>…</span>`
    (`display: block`). An indented line is shifted by N indent steps, and a
    line too long for the column wraps with a hanging indent, as on
    dhammatalks.org (§6a, examples H and I). The exact CSS goes in
    `assets/sass/` and is measured against those pages. Space that follows the
    last line, which joins the segment to the next, is kept after the last span.
37. A verse segment with no `\n` inside it and no leading tab renders
    **byte-identical** to today. Every text without line data (Sujato, the
    Pāli, all SuttaCentral translations) is therefore unaffected; a test
    enforces it.
38. Footnote markers are inserted before the lines are split.
    `footnotes::word_spans()` splits words on `char::is_whitespace()`, so `\n`
    and `\t` do not change a note's `word_index`, and a marker stays at the
    end of its word, inside its line.
39. The stored text does not change. `content_json` keeps `\n` and `\t`;
    `content_plain` treats them as whitespace (checked by a test: no tab in
    `content_plain`); `/sutta.json` and `/sutta_bilara.json` return them
    verbatim, and the API doc (`docs/simsapa-cli-and-localhost-api.md`, §13.4
    and §13.5) states what they mean.
40. Prose segments (template without `verse-line`) are unchanged: a `\n`
    inside them still collapses into a space (§5).

### H. Ṭhānissaro's indentation, in suttas-core (upstream)

41. suttas-core must carry Ṭhānissaro's verse indentation into its segment
    text, in the form of FR 35. Following its own rules (`AGENTS.md`: a
    systematic rule belongs in the parser; builds are deterministic and call no
    model):
    - The `than` parser (`src/sources/than/page.ts`) keeps each verse line's
      indent from the class of its `<p>` (`v1`…`v8`; an unclassed `<p>` is the
      base level) as leading tabs on that line of the raw text. Today it trims
      the indentation (`docs/parsers/than.md`, "Status"). The class-to-level
      mapping is fixed by comparing with dhammatalks.org's rendering (in Thag
      10.5 the `v2` lines are one step in and the `v4` lines two), and is
      written into `docs/parsers/than.md`.
    - `textHash` collapses whitespace, so adding tabs to the raw text makes no
      segmentation stale and needs no model run.
    - The build gives each segment line that **starts** a raw verse line that
      line's tabs, matching on letters and digits, as footnote placement does.
      A segment that starts in the middle of a raw line gets none. Lines that
      cannot be matched are reported by the build, not guessed.
    - It applies to every Ṭhānissaro text with verse (Dhp, Thag, Thig, Snp, Ud,
      Iti, and the verses inside suttas).
42. In Simsapa, the new suttas-core database goes into a new dated folder
    under `suttas-core-db/`, `current` is repointed, and the bootstrap is
    re-run. `SuttasCoreImporter` copies segment text as it is
    (`consistent_niggahita()` only); a test confirms that leading tabs survive
    the import.
43. FR 34–40 do not wait for FR 41: line breaks work as soon as the renderer
    change ships, with no indentation until the new suttas-core database is in
    place. The Munindo and Nyanadipa work does not depend on FR 41.

## 5. Non-Goals (Out of Scope)

- Verse numbers for other languages' Dhammapada translations, and for the Sutta
  Nipāta (Pāli, Sujato or Nyanadipa).
- Changing Munindo's or Nyanadipa's wording, punctuation or line order, apart
  from recorded corrections of clear mistakes (e.g. a verse attached to the
  wrong Pāli verse).
- Importing book-wide front matter (prefaces, general introductions, copyright
  pages) into sutta records.
- Using the Nyanadipa `texts/` folder (non-SuttaCentral numbering).
- Line breaks in prose segments (§6a examples E–G). A `\n` inside a prose
  segment keeps collapsing into a space.
- Indentation for Bodhi, Munindo and Nyanadipa: their sources carry none
  (Munindo's and Nyanadipa's Markdown have no indented lines; Bodhi's
  suttas-core text has no indent data).
- Joining a translator's line that is split across two segments (Ṭhānissaro's
  `dhp190:1` ends `…Dhamma,\n& ` and `dhp190:2` begins `Sangha for refuge,`).
  Each verse segment starts on a new line, so the two halves show as two
  lines. This is how the segmented data is cut, not a rendering fault.
- Any other change to the reader's rendering code.
- A DB version bump or release (the maintainer does that).
- Contributing the segment files to suttas-core.

## 6. Design Considerations

- In the Columns / Lines layouts, the range appears on its own line above the
  verse group in each English column, next to the Pāli
  `183–185. Ānandattherapañhavatthu`. In Solo layout with an English text the
  reader sees:

  ```
  183–185.
  The non-doing of any evil,
  the performance of what's skillful,
  the cleansing of one's own mind:
  this is the teaching of the Awakened.
  ```

- The heading segments are wrapped in `<h2 class='sutta-title'>` by the Pāli
  template, so the range will look like a verse-story heading in every column.
  Check that this reads well in all three layouts; changing its styling is out
  of scope unless it is unreadable.
- Line breaks inside a segment (`\n`): see §6a. **They are not rendered
  today**, for any text.

## 6a. Line breaks inside a segment: examples

**Decided: option 4, rendering in Rust, extended with indentation
(FR 34–43).** Ṭhānissaro's, Bodhi's and Munindo's verse all need the line
breaks, and Ṭhānissaro's verse also needs its indentation (examples H and I).

A Bilara segment is one Pāli line or clause. A translator often uses more or
fewer lines than the Pāli, so the segmented data keeps the translator's own
line breaks as `\n` inside a segment. Simsapa stores them unchanged in
`content_json`, but **nothing renders them**: there is no `\n` → `<br>`
conversion in `bilara_text_to_segments()` / `bilara_multi_column_html()`
(`backend/src/helpers.rs`), and no `white-space` rule on `.verse-line`
(`assets/sass/_suttacentral.sass`) or on the column cells. HTML collapses a
`\n` into a space, so every such segment is shown as one run-on line in the
Solo, Lines and Columns layouts alike.

### How often it occurs (the suttas-core records in the current `appdata.sqlite3`)

Counted per segment. "Verse" / "prose" is the Pāli template's markup for that
segment (`<span class='verse-line'>` or not). "Trailing only" means the
segment ends with `\n` and has none inside.

| Records | `\n` inside, verse | `\n` inside, prose | of which a blank line (`\n\n`) | trailing `\n` only |
|---|---|---|---|---|
| Ṭhānissaro | 2,853 | 264 | 32 | 7,152 |
| Bodhi | 99 | 93 | 0 | 3,221 |
| PTS translators | 13 | 0 | 0 | 69 |

Sujato's and the other SuttaCentral translations have no `\n` at all (they
are cut one line per segment by their translators).

### Examples where the break carries the translation's verse lines

**A. Ṭhānissaro, Dhp 187** (`dhp179-196/en/thanissaro`). Four Pāli lines,
seven English lines. He breaks the verse where his English needs it:

```
dhp187:3  "He is\none who delights\nin the ending of craving,\n"
dhp187:4  "a disciple of the Rightly\nSelf-Awakened One."
```

Shown today:

```
He is one who delights in the ending of craving,
a disciple of the Rightly Self-Awakened One.
```

With the breaks rendered (his printed layout):

```
He is
one who delights
in the ending of craving,
a disciple of the Rightly
Self-Awakened One.
```

**B. Ṭhānissaro, Sn 4.2 verse 1, line 4** (`snp4.2/en/thanissaro`). One Pāli
line, *Kāmā hi loke na hi suppahāyā*, rendered as four short lines:

```
snp4.2:1.4  "for sensual pleasures\nsensual desires\nin the world\nare not lightly let go."
```

Today: `for sensual pleasures sensual desires in the world are not lightly let
go.` Without the breaks, *sensual pleasures sensual desires* reads like a typo,
where his layout sets them on separate lines as a pair.

**C. Bodhi, SN 1.75** (`sn1.75/en/bodhi`). Bodhi capitalises each verse line,
so two lines in one segment show a capital letter mid-line:

```
sn1.75:1.4  "On what should one take a stand\nTo have no fear of the other world?”"
```

Today: `On what should one take a stand To have no fear of the other world?”`

**D. Munindo, Dhp 184** (planned, `dhp179-196/en/munindo`). Munindo's verse is
seven lines in a different order from the Pāli's four. Following the
placement rules (FR 5: the author's order, the lines spread over the four
segments), the file would hold:

```
dhp184:1  "A renunciate does not\noppress anyone.\n"
dhp184:2  "Patient endurance\nis the ultimate asceticism.\n"
dhp184:3  "Profound liberation,\nsay the Buddhas,\n"
dhp184:4  "is the supreme goal."
```

Without the breaks, his short-line style, which is the character of *A
Dhammapada for Contemplation*, becomes four ordinary lines:
`A renunciate does not oppress anyone.` / `Patient endurance is the ultimate
asceticism.` / … Every Munindo verse with more lines than the Pāli is
affected, which is most of them. Nyanadipa's verse is mostly one English line
per Pāli line, so only some of his verses are affected.

### Examples in prose

**E. Ṭhānissaro, AN 10.99, segment 30.1** (`an10.99/en/thanissaro`). One Pāli
segment holds the end of a narration and three turns of dialogue; each `\n` is
one of his paragraph breaks:

```
an10.99:30.1  "“And further, … neither pleasure nor pain.\n“What do you think, Upāḷi? Isn’t this dwelling more excellent …?”\n“Yes, lord.”\n“It’s when seeing this quality …"
```

Today the question and the answer `“Yes, lord.”` run on inside one paragraph.
Rendering the breaks would show the dialogue as he printed it; it would also
give a break inside a `<p>` the Pāli template treats as one paragraph.

**F. Ṭhānissaro, MN 107, segment 1.4**: one `\n` between two sentences,
`…he sat to one side.\nAs he was sitting there, he said to the Blessed One,`.
A break here is his paragraph division; leaving it run-on loses little.

**G. Blank lines (`\n\n`).** 32 Ṭhānissaro prose segments hold a blank line,
e.g. `an1.51:1.1`. Rendering it would give an empty line inside a paragraph.

### Indentation (Ṭhānissaro)

Ṭhānissaro indents verse lines as part of the poem's form. dhammatalks.org
marks each line's level with a class on its `<p>` (`v2` … `v8`; counted over
the vendored pages: `v2` 2,843, `v4` 482, `v3` 392, …), and suttas-core's
parser drops it today.

**H. Dhp 186–187**, as he prints it:

```
Not even if it rained gold coins
would we have our fill
of sensual pleasures.
        ‘Stressful,
        they give little enjoyment’–
knowing this, the wise one
        finds no delight
even in heavenly sensual pleasures.

He is
        one who delights
        in the ending of craving,
        a disciple of the Rightly
        Self-Awakened One.
```

In the segmented data (`dhp187:1` `"finds no delight\n"`), the indent of
*finds no delight* and of the four lines after *He is* is gone, so the line
breaks alone would still not reproduce the poem.

**I. Thag 10.5**, two indent levels:

```
Covered with ignorance,
the body’s tied down with a four-fold tie,¹
        sunk in the floods,²
        caught in the net of obsessions,³
        …
That’s how the body functions,
compelled by the compulsion of kamma,
        but its attainment ends
                in ruin.
        Its many becomings go
                to ruin.
```

The source has `<p class="v2">but its attainment ends</p>` and
`<p class="v4">in ruin.</p>`. The footnote markers sit at line ends, inside
indented lines, so marker placement and indentation must work together
(FR 38).

### Trailing `\n` (must change nothing)

10,000+ segments end with one `\n` and have none inside, e.g. Bodhi
`sn1.1:2.1` `"“After a long time at last I see\n"`. The template already puts
each verse segment on its own line (`.verse-line { display: block }`), so any
solution must render a trailing `\n` as nothing: no extra blank line after
every verse line.

### Options

1. **CSS on verse markup only.** `white-space: pre-line` on `.verse-line`
   and on the verse cells of the Lines / Columns views. Fixes A–D and every
   existing verse segment in one place; leaves prose (E–G) as today. Needs a
   check that a trailing `\n` at the end of a `display: block` span adds no
   line, in Qt WebEngine and in the mobile `QtWebView`, before it is relied on
   (planned test, not yet run).
2. **CSS on all segment text.** As 1, also on prose. Fixes E–F too, but
   introduces blank lines for G and breaks inside template paragraphs.
3. **Convert at import, for Munindo and Nyanadipa only.** A `\n` inside a
   segment becomes `<br>` in their `content_json`. Fixes D only; A–C stay
   run-on; `/sutta.json` would return `<br>` in these two texts.
4. **Render-time conversion in the Rust renderer** (`\n` inside → `<br>`,
   trailing dropped), for verse segments only. Same coverage as 1, with the
   rule in Rust instead of CSS, so it is unit-testable and independent of the
   browser engine; the stored `content_json` stays unchanged.

**Chosen: option 4**, for verse segments, with the indentation of FR 35–36.
Rendering in Rust keeps the stored data unchanged, is unit-testable without a
browser, and serves Qt WebEngine and the mobile `QtWebView` from the same
HTML. Prose (E–G) stays as today.

## 7. Technical Considerations

- **Order of bootstrap steps.** SuttaCentral import → dhammatalks.org →
  suttas-core → Dhammapada / Nyanadipa / Munindo. The Munindo and Nyanadipa
  importers can therefore read the Pāli template from the database. The range
  function needs the Pāli segment ids and texts; the SuttaCentral importer has
  them from the root file, and the later importers can read them from the
  `…/pli/ms` record.
- **Template keys.** `content_json` must have every template key in template
  order (`docs/suttas-core-import.md` §2). A missing key drops that entry's
  markup (`<section>`, `<h2>`, a closing `</p>`).
- **Trailing whitespace** in segments separates words when segments are joined;
  keep it. Lines ending inside a segment use `\n`.
- **Never re-sort `content_json`** keys.
- **Navigation tree and related translations.** The uids do not change, so the
  navigation tree needs no change. `columns_can_align()` will now consider
  these records alignable with the Pāli, because they share the `ms` family.
- **Search.** `content_plain` changes (no HTML wrapper, ranges added in the
  Dhammapada), so the Tantivy index for English and Pāli must be rebuilt in the
  normal bootstrap.
- **Writing the segment files** is a careful reading task done during
  implementation (by an agent, checked line by line by the check command and the
  review report). The files are data, kept in the bootstrap assets folder, not
  generated at bootstrap time.
- **Hashes.** Use one stable hash for `input_hash` / `pali_hash`, documented in
  the doc; suttas-core uses a 16-hex-digit hash, which can be matched.
- **Source data rule.** Defects in the translators' texts are reported in the
  review report, never silently repaired; a correction is made only where the
  verse is clearly attached to the wrong Pāli text, and is always listed.

- **Verified during review (no change needed):** no Ṭhānissaro note and no
  Sujato comment sits on a Dhammapada heading segment; every heading key
  exists in Sujato's and Ṭhānissaro's texts; nothing later in the bootstrap
  rewrites the Pāli or Sujato `content_json`; the gloss data bank's context
  hashes are unaffected, because `paragraphs_from_content_json()` skips
  group-`0` heading segments; Nyanadipa's note references (`Sn 4:5`,
  `SN 22:62`, `MN 140`) resolve through `link_sutta_refs()`, while `Sn 4:17`
  and `Sn 204` (a verse number) stay text.
- **Cost:** the bootstrap parses a few dozen extra JSON maps; the renderer's
  line split runs only on segments that contain a `\n` or a leading tab, and
  others take the existing path unchanged.
- **Cross-project dependency:** FR 41 is a change in the suttas-core
  repository (`git.unxr.net/adicco/suttas-core`), done under its own
  `AGENTS.md`, then a database regeneration. Simsapa's part ships without it.

## 8. Success Metrics

- Ṭhānissaro's Dhp 186–187 and Thag 10.5, and Bodhi's SN 1.75, read with the
  same lines (and, for Ṭhānissaro, the same indents) as the published pages,
  in the Solo, Lines and Columns layouts.
- Opening `dhp1-20/pli/ms` and `snp4.2/pli/ms` with no columns requested shows
  Sujato's English beside the Pāli.

- 26 / 26 Munindo and 36 / 36 Nyanadipa records have `content_json` and no
  `content_html` after a bootstrap.
- The check command reports zero problems for all 62 files.
- 100% of Dhammapada heading segments in the Pāli, Sujato, Ṭhānissaro and
  Munindo records carry their range; the ranges cover verses 1–423 without
  gaps or overlaps.
- Every mismatch found during verification has an entry in the review report.
- 26 / 26 Munindo, 26 / 26 Ṭhānissaro Dhammapada and 36 / 36 Nyanadipa
  records have a non-empty page-title segment.
- The maintainer opens three Munindo chapters and three Nyanadipa suttas in the
  Columns layout and finds the lines aligned with the right Pāli verses.

## 9. Open Questions

None at present.

### Resolved

- Line breaks inside a segment: rendered by Rust for verse segments, with
  Ṭhānissaro's indentation carried by suttas-core as leading tabs (§6a,
  FR 34–43).
- Default English beside the Pāli: stays Sujato (FR 15a).
- Ṭhānissaro's existing `0.3` titles: all 26 get `<n>. <title>`, using his
  existing text where present (FR 24a).

- Ṭhānissaro's Dhammapada chapter titles: filled in the page-title segment
  the same way as Munindo's, `14. Awakened` (FR 24a–24b).
- Nyanadipa's titles: the short Markdown heading (`The Cave`), checked to
  name the correct Pāli sutta (FR 8, FR 12).
- Footnote storage: a separate `bilara/footnotes.json` per source, with the
  fields of a suttas-core `segment_footnotes` row (FR 10).
- Introductions: neither source has chapter- or sutta-level introductions;
  `description` stays supported but empty (FR 11).
- Range separator: en dash, `183–185.` (FR 18, 21).
- Title placement: the template's page-title segment, following Sujato
  (`dhp179:0.3` = `14. The Buddha`, `snp4.2:0.2` = `The Cave`) (FR 8).
