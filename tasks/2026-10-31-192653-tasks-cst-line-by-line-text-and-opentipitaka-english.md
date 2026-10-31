# Tasks: Line-by-line CST text and OpenTipitaka English

PRD: [2026-10-31-192653-prd---cst-line-by-line-text-and-opentipitaka-english.md](./2026-10-31-192653-prd---cst-line-by-line-text-and-opentipitaka-english.md)

## Relevant Files

Bootstrap:

- `bootstrap_cli/src/bootstrap/tipitaka_xml.rs` - The CST importer. Most of tasks 3, 4 and 5 are here, with its tests.
- `bootstrap_cli/src/bootstrap/cst_coverage.rs` - Gains the row-cutting function. The XML splitter is removed.
- `bootstrap_cli/src/bootstrap/navigation_tree/cst_spine.rs` - Keeps calling `fragment_uid`, and supplies `TocLeaf` and `read_toc_leaves`.
- `bootstrap_cli/src/bootstrap/suttacentral.rs` - Reference only: `bilara_text_to_sutta` and `import_sutta_variants` show the pattern to follow.
- `bootstrap_cli/src/main.rs` - `import_cst()` must clear the English and variant rows too.
- `bootstrap_cli/Cargo.toml` - Needs `indexmap`, to parse JSON without losing key order.
Backend:

- `backend/src/opentipitaka.rs` (new) - OT constants and the footer markup, shared by the import and the reader.
- `backend/src/lib.rs` - Registers the new module.
- `backend/src/sutta_display.rs` - Gains `columns_can_align` and `pali_uid_for_translation`, with unit tests.
- `backend/src/app_data.rs` - `get_pali_for_translated`, the default English column, the Lines-mode drop, `render_content_block_for_columns`.
- `backend/src/db/appdata.rs` - `TranslationData.has_content_json`, the column options, `sort_suttas`, `get_full_sutta_uid`, `get_sutta_by_range`.
- `backend/src/query_task.rs` - The two SQL commentary filters.
- `backend/src/search/searcher.rs` - The Tantivy commentary filter.
- `backend/src/sutta_json.rs` - The `/pli/ms` variants fallback.
- `backend/src/cst_files.rs` - Review only.
- `backend/src/helpers.rs` - `related_sutta_uids`, `bilara_multi_column_html` and `bilara_text_to_html`: tests, and no change expected.
- `backend/src/navigation_tree.rs`, `backend/src/navigation_tree/tests.rs` - Translation counts.
Front end:

- `src-ts/column_bar.ts`, `src-ts/column_bar.test.ts` - Reads the relative `has_content_json`, and the disabled-option tooltip.
- `assets/sass/_cst.sass` - The `.cst .centre` rule.
- `assets/sass/_suttacentral.sass` - The Lines-mode `.hangnum` margin.
- `bridges/assets/qml/DhammaTextSourcesDialog.qml` - Lists OpenTipitaka.
- `api/src/routes/suttas.rs` - A comment.

Docs:

- `PROJECT_MAP.md`, `CLAUDE.md`
- `docs/sutta-navigation-tree.md`, `docs/sutta-display-settings-and-multi-column-view.md`, `docs/simsapa-cli-and-localhost-api.md`, `docs/text-processing-for-contains-match-and-fulltext-match-search.md`

Read only (the parser, which must not change):

- `../tipitaka-xml-parser/src/opentipitaka.rs`, `../tipitaka-xml-parser/src/fragments_models.rs`, `../tipitaka-xml-parser/src/fragments_schema.rs`
- `../tipitaka-xml-parser/docs/opentipitaka-export.md`

### Notes

- **Never commit a `*.sqlite3` file**, and commit only when the maintainer says so.
- **This feature ships with a new app version and a new database version.** `appdata.sqlite3` is kept across app updates, so existing installs get the data only through the database version. **Version bumps are the maintainer's**: no task here changes them.
- No migration is needed: the sutta columns and the variants table already exist.
- **The backend tests run against the dist database**, which holds the old HTML records until a new bootstrap. New tests must use their own fixture, and must not rely on segmented CST rows being in the dist database.
- **Never write to the dist `appdata.sqlite3`.** Run `import-cst` on a copy.
- **`build_bilara_text` writes JSON from `IndexMap`, because key order is reading order.** Never pass this JSON through `serde_json::Value` or sort it. `preserve_order` stays off.
- Use `try_exists()`, never `.exists()`. No `println!` in `backend/`. Log with `logger`.
- Timed test failures in `cargo test` are known drift, not regressions.
- Run tests with `cd bootstrap_cli && cargo test`, `cd backend && cargo test`, `cd api && cargo test`, and `npx jest`.
- Docs describe current behaviour only, with no history.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. This helps track progress and ensures you don't skip any steps.

Example:
- `- [ ] 1.1 Read file` → `- [x] 1.1 Read file` (after completing)

Update the file after completing each sub-task, not just after completing an entire parent task.

## Measured on the published database

Checked on 2026-10-31 against `bootstrap-assets-resources/tipitaka-xml-data/fragments.sqlite3`
(574.5 MB, import report `reports/2026-10-31-203806-opentipitaka-import.md`), the dist
appdata of 2026-10-30, and the `romn/` folder.

| Check | Result |
|---|---|
| `cst_segments` | 421,922 rows, 217 files, 16,429 with a variant, 3 with empty `pali_html` |
| `cst_texts` | **7,076** rows in 52 files, 13 empty `en_title` (the 13 in `s0305m.mul.xml`), 1,471 with variants |
| Mapped files | All 52 are on `VERIFIED_CST_FILES`, and every row in them has a fragment |
| `Sutta` fragments with `sc_code` and no `cst_texts` row | 0, and none the other way |
| `moved` fragments | 33, all with no `sc_code` |
| Codes | No duplicates; each equals `cst_text_code` of its fragment |
| Keys | In every text, Pāli, English and both templates have identical keys in identical order, one per row, all `<code>:<n>.<n>` |
| Templates | Exactly one `{}` each; the first opens the `<article>`; the last ends as FR-13 and FR-26 expect |
| Variants | All keys are in the text; `.att` and `.tik` have none |
| `ot_id` 0 chapter rows | Exactly the two in `e0606n` and `e0607n` |
| Division rule (FR-8) | All 165 unmapped files fit: 100 with equal counts, 40 with one more leaf, 24 single-leaf files, and `abh05t.nrf.xml`, the only mismatch. No empty division |
| Division titles | Every division's first chapter row matches its record's toc title |
| Chapter rows' English | Never empty, no `<` or `&`, at most 119 characters |
| Letters, compared with today's `content_plain` | Identical in 7,230 of 7,235 comparisons. The other 5 are the `sn24.11-18` and `e060*` cases below |
| Wrappers | `<p class='hangnum'>` on 58,687 rows, each just a paragraph number in Pāli and English |
| `en_title` | 65 contain `ṃ`, and none contains `&` or `<` |

Expected differences from the dist build, none of them a defect:

- **Uids:** `sn35.32-62.att/pli/cst` is gone. `sn35.32.att`, `sn35.33-42.att`, `sn35.43-52.att`,
  `sn35.53-62.att`, `sn24.19-96.att` and `sn24.19-96.tik` are new, each with `/pli/cst`.
  This takes the records from 8,947 to 8,952.
- **Text:** `sn24.11-18.att` and `.tik` are about 730 letters shorter each, because that
  text is now in the `sn24.19-96` records.
- **Text:** `e0605n`, `e0606n` and `e0607n` gain about 11 letters: the title row at `ot_id` 0.

Also noted:

- **24 single-leaf files have a `<file>.xml` uid** (`e0604n.nrf.xml`, `s0514a2.att.xml` and
  others). They take the division path, not the whole-file path. Only `abh05t.nrf.xml`
  takes `import_whole_file`.
- **Two English rows end in stray JSON, `}, {`** (`s0501t.nrf.xml` row 1288, `s0506a.att.xml`
  row 1012). It cannot break a template, because it is not `{}`. Correcting the English is
  out of scope, so it is shown as it is.
- **`§` separates verse lines in 17 variants.** The 16 records holding them have it in
  `content_plain` today too, so search behaves the same. Do not build a test phrase across it.

## Tasks

### Specs for 1.0

Depends on: nothing.

- The published data is in `../bootstrap-assets-resources/tipitaka-xml-data/fragments.sqlite3`. Only read it, as the importer does.

- [ ] 1.0 Prepare the test fixture
  - [ ] 1.1 Read `../tipitaka-xml-parser/docs/opentipitaka-export.md` from the top through "Measured on the real data".
  - [x] 1.2 The maintainer rebuilt the fragments database in the bootstrap assets folder.
  - [x] 1.3 Confirmed: 421,922 rows in 217 files in `cst_segments`, and 7,076 rows in `cst_texts`.
  - [ ] 1.4 Add a `#[cfg(test)]` fixture helper to `tipitaka_xml.rs`. In a `tempfile` folder, it creates a fragments database, a `romn/` folder, a toc folder and an appdata database (through `bootstrap::run_migrations`). Create the fragments tables with the parser's own `migrations/` SQL, so the schema cannot drift.
  - [ ] 1.5 Give the fixture one fully mapped file with a `moved` fragment, one unmapped file with a head and two chapters, and one file whose toc does not match, which takes the whole-file path. Include a row with a variant, and a row with empty `pali_html` and a variant. Do not use this fixture in any `make` target.

### Specs for 2.0

Depends on: nothing. This is a pure function, tested with no database.

``` rust
pub struct RowDivision<'a> {
    pub division_file: String,
    pub title: String,
    pub rows: &'a [CstSegmentRecord],  // or an owned Vec if the head makes a slice awkward
}
pub fn split_rows_into_divisions<'a>(
    rows: &'a [CstSegmentRecord],
    leaves: &[TocLeaf],
) -> Result<Vec<RowDivision<'a>>, SplitError>
```

A slice works in both cases: a prepended head is simply the first division starting at row 0.

- [ ] 2.0 Add the row-cutting function to `cst_coverage.rs`
  - [ ] 2.1 Add `is_division_row(row: &CstSegmentRecord) -> bool`: true when `pali_wrapper` contains `class='chapter'` and `ot_id != 0`. Comment why: the `rend` column is NULL on these rows in `e0606n` and `e0607n`, and `ot_id` 0 is an extra title that repeats the marker in `e0606n.nrf.xml` and `e0607n.nrf.xml`.
  - [ ] 2.2 Add `split_rows_into_divisions`. Rows before the first marker are the head. Equal counts: prepend the head to the first division. One more leaf: the head is its own division. Anything else: `SplitError::CountMismatch`.
  - [ ] 2.3 Return `SplitError::NoBody` when a division would have no rows, for example a head division with an empty head, so `build_bilara_text` never gets an empty slice.
  - [ ] 2.4 Unit test: equal counts with a head, equal counts with no head, one more leaf, a mismatch, an empty head where the head should be a division, and a chapter row at `ot_id` 0 that does not cut.
  - [ ] 2.5 Run `cd bootstrap_cli && cargo test cst_coverage`.

### Specs for 3.0

Depends on: 1.0 for the fixture.

- [ ] 3.0 Add the input check and carry the new fields through `SuttaRecord`
  - [ ] 3.1 Add `check_fragments_input(&self, sources: &[String]) -> Result<()>`. It errors if the database is absent, if `cst_segments` or `cst_texts` is missing (read `sqlite_master`), or if any source file has no rows in `cst_segments`. Name the missing files in the error, and say to run `import-opentipitaka --fragments-db <path>` in `tipitaka-xml-parser`.
  - [ ] 3.2 Call it at the top of `import()`, before anything is imported.
  - [ ] 3.3 Change `load_coverage`: an absent database is now an error. Update its doc comment, which calls an absent database "the correct outcome". Leave `load_fragments` in `cst_spine.rs` as it is: `build-navigation-tree` is a separate command.
  - [ ] 3.4 Change `source_files`: an absent `romn/` directory is now an error, as is a directory with no `.xml` files. Remove the `sources.is_empty()` early return in `import()`.
  - [ ] 3.5 Add `content_json`, `content_json_tmpl`, `license`, `source_language` and `variant_json` to `SuttaRecord`, all `Option<String>`. Set `source_language` to `Some("pli")` at the three existing call sites for now.
  - [ ] 3.6 Make `insert_suttas` read those fields from the record, in place of the hard-coded values.
  - [ ] 3.7 After each sutta insert, if `variant_json` is set, insert a `NewSuttaVariant` with the new `sutta_id`, `sutta_uid`, `language` `pli`, `source_uid` `cst`, in the same transaction. Read the id with `RETURNING`, or a uid lookup as `import_sutta_variants` does. A uid that was skipped as existing gets no variant row.
  - [ ] 3.8 Test: each error of 3.1 and 3.3 stops the import with nothing inserted, and the message names the command.
  - [ ] 3.9 Run `cd bootstrap_cli && cargo test`.

### Specs for 4.0

Depends on: 2.0, 3.0.

One internal shape, so that all three paths build records in one place:

``` rust
struct CstTextParts {
    pali_json: String,
    pali_tmpl: String,
    variant_json: Option<String>,
    en_title: String,        // used by task 5.0
    en_json: String,         // used by task 5.0
    en_tmpl: String,         // used by task 5.0
}
```

Filled from `CstTextRecord` for fragments and from `BilaraText` for divisions and whole files.

`content_plain` is derived from the exported template, before the header is added (FR-16, FR-24).

Header, inserted straight after the `<article id='…' class='cst'>` in the first key's template:

- Fragment: `<header><h3>{nikaya} {number}</h3><h1>{title}</h1></header>`
- Division or whole file: `<header><h1>{title}</h1></header>`

`bilara_multi_column_html` and `bilara_text_to_segments` replace every `{}` in a template, so a title must not add one.

- [ ] 4.0 Import Pāli records as segmented text from all three paths
  - [ ] 4.1 Add `indexmap` to `bootstrap_cli/Cargo.toml`, the same version as `backend`, with the `serde` feature.
  - [ ] 4.2 Change `fragment_uid` to use `cst_text_code`, then `format!("{}/pli/cst", code)`. Run the `cst_spine` tests.
  - [ ] 4.3 Add `escape_title(&str) -> String`: `html_escape::encode_text`, plus `{` to `&#123;` and `}` to `&#125;`.
  - [ ] 4.4 Add `add_header_to_template(tmpl_json: &str, header_html: &str) -> Result<String>`. Parse into `IndexMap<String, String>`, take the first entry, error if it does not start with `<article`, insert after that tag's first `>`, and serialize. Test that key order is kept on keys that sort differently (`mn1:10.1` before `mn1:2.1` in the input).
  - [ ] 4.5 Add `plain_text_of(json: &str, tmpl: &str, variant_json: Option<&str>) -> Result<String>`, to be called with the template before the header is added. Call `bilara_text_to_html` with no variants, comments or glosses, and all flags false, then `sutta_html_to_plain_text`. Then append each variant value, in key order, passed through `compact_plain_text` and joined by a space. Test that the result has no newline, and no `.` or `,` from a variant.
  - [ ] 4.6 In `import_from_fragments`, order by `start_line`, then `start_char`.
  - [ ] 4.7 Load the file's `cst_texts` rows into a `HashMap` keyed by `frag_idx_code`, once per file. Skip a fragment with `frag_review = 'moved'` first, whatever its `sc_code`. A `Sutta` fragment with an `sc_code` and no `cst_texts` row is an error naming the file and code. Also error if `CstTextRecord.code` differs from `cst_text_code`.
  - [ ] 4.8 Replace `xml_to_html` in `import_from_fragments` with the `cst_texts` JSON. Set `content_html: None`. Apply `consistent_niggahita` to the JSON, the template, the variant JSON and the title. Call `plain_text_of`, then add the fragment header.
  - [ ] 4.9 In `import_divisions`, read the file's rows `ORDER BY ot_id` once per file, and call `split_rows_into_divisions`. Keep `read_toc_leaves` for the leaves and `read_xml_file` + `detect_nikaya_structure` for the nikāya. A `SplitError` or a `build_bilara_text` error stays a `DivisionError`.
  - [ ] 4.10 For each division, the `code` passed to `build_bilara_text` is `division_file` without `.xml`. Convert rows with `SegmentRow::from(&record)`. Keep the title rule, uid, `sutta_ref` and indexes. Call `plain_text_of`, then add the division header.
  - [ ] 4.11 In `import_whole_file`, do the same with all the file's rows. The code is `filename` without `.xml`, for example `abh05t.nrf`.
  - [ ] 4.12 Remove `split_into_divisions`, `split_body_at_chapters`, `Division` and every helper that is then unused, with their tests. Remove the `xml_to_html` import. Update `Division` mentions in comments and the module doc table.
  - [ ] 4.13 Test on the fixture, checking the stored rows: `content_json` and `content_json_tmpl` set; `content_html` None; `license` None; uid and `source_uid` unchanged; no `ṃ` in the JSON, variants or title; the header in the first template; a variant row with `sutta_id`, keyed by segment; a `moved` fragment absent; and a missing `cst_texts` row is an error.
  - [ ] 4.14 Test FR-16 and FR-17: the variant text is in `content_plain`, the empty `pali_html` row's variant text is too, a phrase crossing a comma in it matches after query normalization, and the title is not in the Pāli `content_plain`.
  - [ ] 4.15 Test that the key order in the stored JSON equals the order in the source, comparing strings without sorting.
  - [ ] 4.16 Run `import-cst` with the published fragments database on a **copy** of the dist `appdata.sqlite3`. Compare the Pāli uid list with the copy's list before the run: the only differences must be those under "Measured on the published database". `report.fragment_records` must be 7,076.

### Specs for 5.0

Depends on: 4.0.

``` rust
// backend/src/opentipitaka.rs
pub const OT_AUTHOR_UID: &str = "opentipitaka";
pub const OT_ENGLISH_FOOTER: &str = "This English translation was produced by AI (www.opentipitaka.org)";
pub const OT_LICENSE: &str = "CC-BY-NC-SA 4.0";
pub const OT_SOURCE_URL: &str = "https://www.opentipitaka.org/";
pub const OT_LICENSE_URL: &str = "https://creativecommons.org/licenses/by-nc-sa/4.0/";

/// `<footer class='noindex'>Licence: <a>CC-BY-NC-SA 4.0</a>. Source: <a>opentipitaka.org</a>.
/// Arranged in lines for Simsapa, with the niggahīta spelled ṁ.</footer>`, linking to
/// `OT_LICENSE_URL` and `OT_SOURCE_URL`. The import adds this after the exported AI footer.
pub fn ot_license_footer_html() -> String;

/// The AI footer followed by the licence footer. The reader adds this when the
/// English is a column but the opened text is not.
pub fn ot_footer_html() -> String;

/// Whether `uid` is an OpenTipitaka English record.
pub fn is_ot_english_uid(uid: &str) -> bool;
```

- [ ] 5.0 Create the `en/opentipitaka` records and make `import-cst` re-runnable
  - [ ] 5.1 Create `backend/src/opentipitaka.rs` as in the specs, and register it in `backend/src/lib.rs`. Unit test the footers.
  - [ ] 5.2 Add a `bootstrap_cli` test asserting that `OT_AUTHOR_UID`, `OT_ENGLISH_FOOTER`, `OT_LICENSE` and `OT_SOURCE_URL` equal the parser's. `OT_LICENSE_URL` is the backend's own.
  - [ ] 5.3 Add `add_license_to_template(tmpl_json) -> Result<String>`, which appends `ot_license_footer_html()` to the last key's template, keeping key order.
  - [ ] 5.4 Add `english_record(pali: &SuttaRecord, parts: &CstTextParts, title: &str) -> Result<SuttaRecord>`. Clone the Pāli record, then set: uid `<base>/en/opentipitaka`, where `<base>` is the Pāli uid's first path component; `language` `en`; `source_uid` `opentipitaka`; `source_language` `pli`; `license` `OT_LICENSE`; `content_html` None; `variant_json` None; `title_pali` the Pāli title; `title_ascii` from the English title.
  - [ ] 5.5 Build the English JSON with `consistent_niggahita`, and derive `content_plain` with `plain_text_of`, no variants. Then build the template: the same header with the English title in the `<h1>`, then the licence.
  - [ ] 5.6 English title for a fragment: `en_title`, or the Pāli title when empty. Apply `consistent_niggahita` to it, on all three paths.
  - [ ] 5.7 English title for a division: the `english` of the first row where `is_division_row` is true; otherwise, or if that text is empty, the toc's Pāli title. Chapter rows hold no markup or entities, so use the text as it is.
  - [ ] 5.8 English title for a whole file: the Pāli title.
  - [ ] 5.9 Push one English record after each Pāli record on all three paths. A duplicate Pāli uid already skips before this point, so it makes no English record. Keep `report.fragment_records` and `report.division_records` counting Pāli records only, so `CstCoverageReport` keeps its counts. Log the English count separately.
  - [ ] 5.10 Check that the range columns match: `sutta_range_from_ref` on `mn1.att/en/opentipitaka` and `vin02m4.mul0.xml/en/opentipitaka` must equal the Pāli uid's. If not, compute them from the Pāli uid.
  - [ ] 5.11 In `import_cst()` in `main.rs`, delete `sutta_variants` where `source_uid = 'cst'`, then `suttas` where `uid` is like `%/pli/cst` or `%/en/opentipitaka`. Update its doc comment. Log both counts.
  - [ ] 5.12 Test on the fixture: every Pāli record has one English record with identical keys in identical order; every field of FR-19 and FR-20; the Pāli record's `license` is None; the title fallbacks; no `ṃ` in the English title; the English `content_plain` has no "produced by AI" text and no title; and the template's last key holds both footers, with links to the licence and the source.
  - [ ] 5.13 Run `import-cst` twice on the appdata copy, and check that the second run gives the same row counts for suttas and variants.
  - [ ] 5.14 Measure now, while the design can still change: `VACUUM` the copy, then give the maintainer its size and each `suttas` and `suttas_fts_*` size from `dbstat`, against the estimates in PRD §7.

### Specs for 6.0

Depends on: 5.0 for `is_ot_english_uid`. The code compiles and works with the old database, where nothing is in a mixed state.

``` rust
// backend/src/sutta_display.rs

/// The CST family: `/pli/cst` or `/en/opentipitaka`.
pub fn is_cst_family_uid(uid: &str) -> bool;

/// Whether these columns can be aligned on the template of `opened_uid`.
/// Segment keys compare only within one text family, and the aligned view
/// uses the opened text's template, so the opened uid counts as a column.
pub fn columns_can_align(opened_uid: &str, column_uids: &[&str]) -> bool;

/// The Pāli text a translation is read beside. `mn1/en/opentipitaka` gives
/// `mn1/pli/cst`, and every other author gives `<base>/pli/ms`.
pub fn pali_uid_for_translation(uid: &str) -> String;
```

- [ ] 6.0 Guard alignment in the reader
  - [ ] 6.1 Add the three functions, with `is_cst_family_uid` and `columns_can_align` true only when: no uid, opened included, is in the CST family; or all are, with one base.
  - [ ] 6.2 Unit test `columns_can_align`: `mn1/pli/ms` + `mn1/en/sujato` true; `mn1/pli/cst` + `mn1/en/opentipitaka` true; `mn1/pli/cst` + `mn1/pli/ms` false; `mn1/pli/cst` + `mn1.att/en/opentipitaka` false; opened `mn1/en/sujato` with columns `mn1/pli/cst` + `mn1/en/opentipitaka` false; and the `.xml` division uids true.
  - [ ] 6.3 Change `get_pali_for_translated` to query `pali_uid_for_translation(&sutta.uid)`, and correct its comment.
  - [ ] 6.4 In `resolve_sutta_display_options`, Lines-mode drop: keep a column only if it is segmented **and** `columns_can_align(&sutta.uid, &[col_uid])`. The existing "unknown uid is kept" arm stays. An emptied list already falls back to the opened sutta in `resolve_column_suttas`. Test that case.
  - [ ] 6.5 In `render_content_block_for_columns`, require `columns_can_align` as well as `all_segmented` for the aligned branch. Non-alignable columns then reach the existing block-columns branch.
  - [ ] 6.6 Change the `has_content_json` value in `get_translations_data_json_for_sutta_uid`: segmented **and** `columns_can_align(sutta_uid, &[&s.uid])`. Update its doc comment, and the `api/src/routes/suttas.rs` comment.
  - [ ] 6.7 In the aligned branch, if any column satisfies `is_ot_english_uid` and the opened sutta does not, append `ot_footer_html()` to the returned HTML. Do not touch the unaligned or Solo paths, where the English's own template carries the footer.
  - [ ] 6.8 Test, with fixture `Sutta` values and no dist data: the footer appears once in Lines and Columns with the Pāli opened; once with the English opened; in Solo on the English; and not when no OT column is present. Check the block-columns path too.
  - [ ] 6.9 Test that a CST header in the first template renders in the aligned view, and that a template with a `<footer>` after `</article>` keeps it. These are `bilara_multi_column_html` tests in `helpers.rs`.
  - [ ] 6.10 Read `column_bar.ts`, including `ensure_current_columns_present` (`has_content_json: true` at line 97) and line 136. No logic change is expected, since the server's answer is relative. Change `LINES_DISABLED_TITLE` to `Cannot be shown line by line with this text — available in the Columns layout`, which is true of both cases (FR-48). Add `column_bar.test.ts` cases for a CST opened text: another-family option is disabled in Lines and enabled in Columns; and the same-base option stays enabled.
  - [ ] 6.11 In `get_translations_data_json_for_sutta_uid`, keep a CST record in the `include_cst_mula` filter when `columns_can_align(sutta_uid, &[&s.uid])` holds and the opened text is in the CST family (FR-47). Test with `include_cst_mula` false: opened `mn1/en/opentipitaka` lists `mn1/pli/cst`; opened `vin02m4.mul0.xml/en/opentipitaka` lists its Pāli; opened `mn1/en/sujato` does not list `mn1/pli/cst`.
  - [ ] 6.12 Run `cd backend && cargo test`, `cd api && cargo test`, and `npx jest`.

### Specs for 7.0

Depends on: nothing beyond a compiling tree.

- [ ] 7.0 Update ordering and search
  - [ ] 7.1 In `sort_suttas`, change the `remaining` key to `(language, is_ot_english_uid(&uid), range_start, uid)`. Add a test with `en/sujato`, `en/brahmali`, `en/opentipitaka` and a `hu` text. Other languages keep their order.
  - [ ] 7.2 In `filter_suttas_query`, extend the commentary condition with `uid.like("%.att%/opentipitaka")` and `uid.like("%.tik%/opentipitaka")`. Leave the mūla condition unchanged (FR-37).
  - [ ] 7.3 Make the same change in the `extra_where` SQL in `suttas_contains_match_fts5`, and update its comment.
  - [ ] 7.4 In `searcher.rs`, make the commentary exclusion match `source_uid` `cst` **or** `opentipitaka`: two `MustNot` clauses, or one `Should` pair inside a `Must`. Leave the mūla exclusion unchanged, and update the comment.
  - [ ] 7.5 Test the Tantivy filter on a small temporary index, in the style of the tests already in `searcher.rs`: with `include_cst_commentary` false, `mn1.att/en/opentipitaka` is gone, and `mn1/en/opentipitaka` and `mn1/en/sujato` remain.
  - [ ] 7.6 Add `related_sutta_uids` tests for `mn1.att/en/opentipitaka`: with commentary off, opening `mn1/pli/cst` does not list it; opening `mn1.att/pli/cst` does. Confirm `include_cst_mula` leaves `mn1/en/opentipitaka` listed in `get_translations_data_json_for_sutta_uid`, since `ends_with("/cst")` does not match it.
  - [ ] 7.7 Confirm by reading that `uid_sutta_cst_files_page` keeps `%.xml/pli/cst`, so FR-38 needs no change.
  - [ ] 7.8 Run `cd backend && cargo test`.

### Specs for 8.0

Depends on: 6.0 for `pali_uid_for_translation`.

- [ ] 8.0 Update the code that assumes `pli/cst` is not segmented
  - [ ] 8.1 In `sutta_json.rs`, replace the `format!("{}/pli/ms", reference)` in the variants fallback with `pali_uid_for_translation(&sutta.uid)`. Add a test.
  - [ ] 8.2 Correct the `sutta_json.rs` comment that names `pli/cst` as HTML-only, and the one at line 59 if it no longer holds. `get_sutta_content_json` needs no code change: it returns `Found` when `content_json` is set. Check the "bare code resolves to `/pli/ms`" rule stays.
  - [ ] 8.3 Check `cst_files.rs` and `query_task.rs` for that assumption, and update any comments. `grep -rn "content_json" ` and `"pli/cst"` in `backend/src`, `api/src`, `bridges/src` and read each hit.
  - [ ] 8.4 Check the four `content_html` fallbacks in `query_task.rs` (lines 443, 494, 518, 572) still get text: `content_plain` is set on every CST record, and a test with `content_html` None and a filled `content_plain` gives a snippet. Lines 518 and 572 read book spine items, so they are unaffected.
  - [ ] 8.5 Check `app_data.rs` `render_sutta_standard_body`, `asset_helpers.rs:153` and `cli_support.rs:419` handle `None`.
  - [ ] 8.6 Add `navigation_tree/tests.rs` cases: `mn1/en/opentipitaka` counts as a translation of `mn1/pli/cst`, and `mn1.att/en/opentipitaka` does not count for `mn1/pli/cst`, as `a_commentary_is_not_counted_as_a_translation_of_the_text` does for other authors.
  - [ ] 8.7 In `get_full_sutta_uid`, after the `/pli/ms` lookup, try `<code>/pli/cst` before the `LIKE` (FR-49). Test on a bare `mn1.att`-shaped code with only `/en/opentipitaka` and `/pli/cst` rows.
  - [ ] 8.8 In `get_sutta_by_range`, among suffix matches take a mūla record first, using `split_sutta_ref_kind`. With no suffix match, take a Pāli mūla record, then `results.first()` (FR-50). Update the `sutta_json.rs` comment at line 59, whose example changes, and keep `resolve_sutta_strict`'s suffix check.
  - [ ] 8.9 Run `cd backend && cargo test` and `cd api && cargo test`.

### Specs for 9.0

Depends on: nothing.

- [ ] 9.0 Add the display and attribution touches
  - [ ] 9.1 In `assets/sass/_cst.sass`, add `.centre` beside `.centered`, with the same rules. Run `make sass`.
  - [ ] 9.2 In `_suttacentral.sass`, set `.cst .hangnum` to `margin-bottom: 0` under `.suttacentral.layout-lines`. Solo and Columns keep `-18pt`.
  - [ ] 9.3 Read the rest of `_cst.sass` against the exported markup: `<article class='cst'>`, `<p class='bodytext'>`, `<p class='gathalast'>`, and `<br>` in a verse row. Report anything unstyled to the maintainer. Do not restyle it.
  - [ ] 9.4 In `DhammaTextSourcesDialog.qml`, add `<li><a href="https://www.opentipitaka.org/">opentipitaka.org</a> English translation of the CST texts, produced by AI</li>` under the tipitaka.org entry. The link is opened by the existing `onLinkActivated`.
  - [ ] 9.5 Run `make build -B` and `make qml-lint`, and check for no new warning in that file.

### Specs for 10.0

Depends on: 5.0 for the OT uids, 6.0 for the alignment guard, and 7.1 for the sort order.

``` rust
// backend/src/app_data.rs

/// The English shown beside a Pāli text when no columns are requested (FR-45).
/// Reads uids and a non-empty test on `content_json`, never the full rows, with
/// an indexed range: `uid >= 'mn1/en/' AND uid < 'mn1/en0'`.
fn default_english_for_pali(&self, sutta: &Sutta) -> Result<Option<String>>;
```

In `resolve_sutta_display_options`, the variable `pali_uid` becomes the paired uid: the Pāli for a translation, as today, and `default_english_for_pali` for a Pāli text. It is still read only when `overrides.columns` is `None`, so requested columns win. `SuttaDisplayOptions::resolve` needs no logic change. Its parameter and doc comment are renamed.

- [ ] 10.0 Show an English beside an opened Pāli text
  - [ ] 10.1 Add `default_english_for_pali`. A `/pli/cst` text gives `<base>/en/opentipitaka` if it exists, and nothing else. Any other Pāli text gives the first segmented English, then the first non-segmented one, leaving out `opentipitaka`.
  - [ ] 10.2 Use it in `resolve_sutta_display_options`, as in the specs. Do not read `/pli/ms` from it, and keep `get_pali_for_translated` for translations only.
  - [ ] 10.3 Test: `mn1/pli/cst` gives columns `mn1/pli/cst` + `mn1/en/opentipitaka`, aligned, with one footer; `mn1.att/pli/cst` and a `.xml` record do the same; `mn1/pli/ms` gives `mn1/en/sujato`; a Pāli text whose English is not segmented is alone in Lines and a block in Columns; a text with no English is alone; Solo renders the Pāli alone and keeps both columns in `SUTTA_DISPLAY`.
  - [ ] 10.4 Test that `columns=mn1/pli/cst` stays one column. Read `fetch_content_block` in `content_reload.ts`, and confirm that removing the English and switching layout sends the column list, so that the English does not come back while the page is open.
  - [ ] 10.5 Update any test that expects one column for an opened Pāli text, in `backend`, `api` and `backend_cli` (`commands/sutta.rs` uses the same options).
  - [ ] 10.6 Run `cd backend && cargo test`, `cd api && cargo test`, `cd backend_cli && cargo test` and `npx jest`.

### Specs for 11.0

Depends on: 2.0 to 10.0, so the docs state what was built.

- [ ] 11.0 Update the docs
  - [ ] 11.1 `PROJECT_MAP.md`: add `backend/src/opentipitaka.rs`, and change the CST import description.
  - [ ] 11.2 `docs/sutta-navigation-tree.md`: CST records are segmented and have an English record; the rebuild commands now include `index rebuild --area suttas --lang en`; and the translation counts include OpenTipitaka. Change the matching summary in `CLAUDE.md`.
  - [ ] 11.3 `docs/sutta-display-settings-and-multi-column-view.md`: the text-family rule, `columns_can_align` with the opened uid as a member and why, `has_content_json` being relative to the opened text, the same-base option that `include_cst_mula` does not remove, the footer rule, and the default English column for a Pāli text.
  - [ ] 11.4 `docs/simsapa-cli-and-localhost-api.md`: `/sutta.json` and `/sutta_bilara.json` now find CST texts; `has_content_json`'s meaning; the variants rule; and a Pāli page with no `columns` having an English column in Lines and Columns. Keep this doc self-contained: no source paths and no links to other docs. Run `cd backend_cli && cargo test test_bundled_doc_has_no_repo_references`.
  - [ ] 11.5 `docs/text-processing-for-contains-match-and-fulltext-match-search.md`: compacted variants are appended to `content_plain` for CST records, and why, and that the title is left out.
  - [ ] 11.6 `docs/sutta-display-settings-and-multi-column-view.md`: notes as ⧫ marks on CST records.
  - [ ] 11.7 `docs/user-data-and-sqlite-analyze.md` needs no change, since this is bootstrap data. Confirm that, and check any `pli/cst` mention left in `docs/` is still true.

### Specs for 12.0

Depends on: all tasks above.

- [ ] 12.0 Verify on real data, and measure
  - [ ] 12.1 On the appdata copy from 5.13, write the checks as one SQL script in the scratch folder: every `pli/cst` row has `content_json`, `content_json_tmpl` and no `content_html`; every one has exactly one `en/opentipitaka` row; and a small Rust or `jq --stream` check confirms identical keys in identical order on a sample of each path. Do not use a check that sorts keys.
  - [ ] 12.2 Run `cd bootstrap_cli && cargo test`, `make rust-test`, `npx jest` and `make qml-test`.
  - [ ] 12.3 Ask the maintainer to run the full bootstrap, or for the go-ahead to run it.
  - [ ] 12.4 Compare with the previous build: the Pāli CST uid set and `CstCoverageReport` differ only as listed under "Measured on the published database". The working database moved on after the PRD was measured, so the set is not "unchanged".
  - [ ] 12.5 Search the built database through `simsapa` CLI or the API for the phrases in success metric 4, with `--pali-edition both` (`"pali_edition": "both"`) if the edition preference is already in: mūla, `.att`, `.tik` and an unmapped division, in Pāli and English; a variant reading; and the Dhammapada summary at `s0502m.mul.xml` row 905.
  - [ ] 12.6 With commentary off, in Contains and Fulltext modes, confirm no `.att` or `.tik` record is returned in either language.
  - [ ] 12.7 Measure against the previous build, and put the figures in this file: appdata size, `suttas_fts` size (`dbstat`), the `en` and `pli` Tantivy index sizes, the compressed release size, peak memory of the import, and peak memory of `index rebuild --area suttas --lang en`. `records` holds the whole corpus until `insert_suttas`, so if the import's peak is a problem, insert per file instead. The indexer loads every full row of a language, so a high figure for it matters on a phone, where the index can be rebuilt in-app.
  - [ ] 12.8 List these for the maintainer to check by hand in the app, with a built database: `mn1/en/opentipitaka` in Solo, Columns and Lines; `mn1/pli/cst` beside `mn1/en/sujato` never aligned; the `<h3>` over an English title (section 9 of the PRD); the AI footer and licence, and both of their links, in each layout with either text opened first; the Translation tab order; a verse, a `.centre` line and a `.hangnum` row in Lines; a ⧫ mark opening a note, and the Dhammapada summary row; and `mn1/pli/cst` and `mn1/pli/ms` opened in Lines, Columns and Solo, with the English removed and the layout then changed.
  - [ ] 12.9 Give the maintainer the size figures for the size decision in section 9 of the PRD.
  - [ ] 12.10 The maintainer sets the new app and database versions and builds the release, with the language databases and the indexes rebuilt from the new bootstrap. The other two PRDs ship in the same database version, as far as they are finished.
