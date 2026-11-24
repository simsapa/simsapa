# PRD: Thai Tipiṭaka bundle (Siam Rath Pāli, Royal, MCU and Mahamakut translations with commentaries)

## 1. Introduction / Overview

Thai readers have no complete Thai Tipiṭaka in Simsapa. Today the SuttaCentral
Thai download (`suttas_lang_th`) holds 3,234 texts labelled `siam_rath`, which
are in fact the **Royal (ฉบับหลวง) Thai translation** of DN, MN, SN and AN only
(plus one Snp text). It also holds 5 texts by Dhīranando and Jayasāro. There is
no Vinaya, no Abhidhamma, almost no Khuddaka Nikāya, no Thai-script Pāli, and
no Thai commentary.

This feature adds an **optional download, the Thai Tipiṭaka bundle**. It holds
five texts:

| Text | Language | What it is |
|---|---|---|
| Siam Rath Pāli (สยามรัฐ) | Pāli in Thai script | the 45-volume Thai edition of the Pāli canon |
| Royal translation (ฉบับหลวง) | Thai | 45 volumes |
| MCU translation (ฉบับมหาจุฬาฯ, มจร.) | Thai | 45 volumes |
| Mahamakut translation (ฉบับมหามกุฏฯ, มมร.) | Thai | the canon |
| Mahamakut commentary translation (อรรถกถา) | Thai | interleaved with the canon in the 91-volume set |

The bootstrap builds the bundle into its own SQLite database and archive. It is
a separate archive from `suttas_lang_th`, because it also holds the Thai-script
Pāli edition. When it is downloaded, its records are imported into
`appdata.sqlite3`, the same way the per-language translation archives are.

**For the user, the bundle and the Thai language are one selection.** Picking
Thai (`th`) in a language list selects both the SuttaCentral Thai archive and
the Thai Tipiṭaka. Removing `th` removes both. Both the first-run download
window and the Sutta Languages window also show a separate **Thai Tipiṭaka**
checkbox, to promote it. Ticking the checkbox is the same action as selecting
`th`, and the two controls stay in sync. A user gets the same result whichever
one they use.

Thai users expect to read Thai and to search by typing Thai, with Thai search
snippets. This needs a Thai word tokenizer for the fulltext index. Without
one, Thai fulltext search does not work today, not even for the SuttaCentral
Thai texts (§7.6).

**Goal:** a Thai reader can download one optional bundle and then:

- read the whole canon in Thai-script Pāli and in three Thai translations;
- open the Thai commentary from a sutta;
- search in Thai;
- look up any Siam Rath Pāli word in DPD.

Every record is placed under the uids the app already uses.

## 2. Goals

1. **Bootstrap.** Produce `suttas_bundle_thai_tipitaka.tar.bz2`, which holds:
   - the bundle database;
   - its prebuilt fulltext indexes;
   - Siam Rath Pāli, Royal, MCU and Mahamakut records under existing SuttaCentral uids (`mn1/th/royal`, `pli-tv-bu-vb-pj1/th/mcu`, `mn1/pli/siam`);
   - Mahamakut commentary records under the CST commentary uids (`mn1.att/th/mbu`, `an1.1-10.att/th/mbu`).
2. **Alignment report.** Measure the alignment of every edition to the uids and write it to a report.
   - Include a collection of an edition only where the alignment passes the acceptance checks (§4.3).
   - Report every disagreement between SuttaCentral's `sya` references and the Thai text, and never repair it.
3. **Thai is one selection.** Selecting `th`, by typing it or by clicking it in the language list, and ticking the Thai Tipiṭaka checkbox are the same action, with the same result:
   - it applies in `DownloadAppdataWindow.qml`, in the Sutta Languages window, and in `simsapa setup`, which must change together with the download window;
   - it downloads `suttas_lang_th` and the bundle, and imports both into appdata;
   - it is kept across a database upgrade, the way languages are kept.
4. **Removal.** In the Sutta Languages window, removing `th`, or ticking "Remove Thai Tipiṭaka", removes both archives' records and index folders. It touches no other Pāli records.
5. **Thai fulltext search.** Thai search finds Thai words and shows Thai snippets with highlights, for the bundle and for the SuttaCentral `th` texts.
6. **Pāli lookups from Thai script.** Selecting a word in a Siam Rath Pāli text romanizes it before a DPD lookup, gloss or word-summary lookup, so the Pāli tools work on Thai-script Pāli.
7. **Commentary buttons.** The reader's m | a | t buttons and the translations list open the Thai commentary from a Thai sutta record. The uid scheme makes it a candidate. One added tier in the source preference (item 11) makes it the one chosen.
8. **Search options.** Siam Rath is a third Pāli edition beside `ms` and `cst` in the advanced search options, with a "Siam Rath Mūla in Search" checkbox. The "Text in both editions" dropdown stays MS / CST / Both (item 46 explains why).

## 3. User Stories

- As a Thai reader, at first-run download I tick **Thai Tipiṭaka**. `th`
  appears in the language selection by itself. When the download finishes I
  can read MN 1 in the Royal, MCU and Mahamakut translations and in Siam Rath
  Pāli.
- As a Thai reader who doesn't notice the checkbox, I type `th` in the
  language box. The Thai Tipiṭaka checkbox ticks itself, and I get the same
  download.
- As a Thai reader of `mn1/th/royal`, I press **a** and the Mahamakut Thai
  commentary on MN 1 opens.
- As a Thai reader, I type `ปฏิจจสมุปบาท` in sutta search. I get Thai results
  with the word highlighted in the snippets.
- As a Thai reader of `mn1/pli/siam`, I select `ปุถุชฺชโน`. The word summary
  shows DPD's `puthujjana`.
- As a Thai reader, I untick "Siam Rath Mūla in Search" to see only the Thai
  translations among my Thai-script results. Ticked, a Thai-script Pāli query
  finds the Siam Rath texts whatever "Text in both editions" is set to.
- As a reader who installed Thai earlier, I remove `th` in the Sutta Languages
  window, or tick "Remove Thai Tipiṭaka". All Thai texts and the Siam Rath
  Pāli go. The MS and CST Pāli texts stay.
- As a user who skipped Thai at first run, I add it later from the Sutta
  Languages window, by the checkbox or by selecting `th`.
- As the maintainer, I read the alignment report after a bootstrap. I see per
  collection and per edition how many suttas were placed, how, and what was
  left out. I see a list of SuttaCentral `sya` references that disagree with
  the Thai text, to send upstream.

## 4. Functional Requirements

### 4.1 Source data

1. Use [kit119/D-tipitaka](https://github.com/kit119/D-tipitaka) release 1.2 (`1.2/*.sql.bz2`). Its four tables come from the E-Tipitaka 2.1.0 CD:
   - `pali_siam`: 45 volumes, 21,915 pages;
   - `thai_royal`: 45 volumes, 19,697 pages;
   - `thai_mcu`: 45 volumes, 25,155 pages, with `header` / `footer` / `display` columns;
   - `thai_mbu`: 91 volumes, 62,380 pages; `volumn_orig` maps each to its 45-volume number (the column name's misspelling is in the source and is kept verbatim).
2. Store the four `.sql.bz2` files unchanged in `bootstrap-assets-resources/thai-tipitaka/`, with a `README.md` that records:
   - the upstream URL and the pinned commit SHA;
   - the sha256 of each file.

   The bootstrap loads them into a temporary SQLite database (`bunzip2 | sqlite3`, or the equivalent in Rust).
3. Write the folder path once, as `THAI_TIPITAKA_DIR` in `bootstrap_cli/src/bootstrap/source_paths.rs`. Add the four files to `bootstrap_sources()` in `preflight.rs` as **required whenever `th` is selected**. Run `cargo test test_full_bootstrap_sources -- --ignored`.
   - **Why required and not optional.** Item 10 leaves SuttaCentral's Royal texts out of `suttas_lang_th`, and the app always fetches the bundle with `th`. A run that skipped the bundle would ship a `th` that lost the Royal translation, and an app that asks for a bundle archive the release does not have.
   - For the same reason, item 10's exclusion applies only when the bundle is built in the same run.
4. **Report source defects, never repair them** (standing rule). Typos, missing paragraphs and wrong numbers in D-tipitaka, and wrong `sya` references in SuttaCentral, are kept and listed in the report (§4.3).
   - Layout normalization is not a repair and is required: joining pages, joining a word broken across lines (`ของ-\nพระอริยะ` → `ของพระอริยะ`), collapsing runs of justification spaces to one space, and converting Thai numerals in anchors.
   - Keep single spaces: in Thai they separate phrases.
5. **Licence and attribution.** These translations are already widely distributed (84000.org, E-Tipitaka). Every record carries attribution in `source_info` / `copyright`:
   - Royal: Department of Religious Affairs (กรมการศาสนา), B.E. 2525;
   - MCU: Mahachulalongkornrajavidyalaya University;
   - Mahamakut: Mahamakut Buddhist University;
   - Siam Rath: the B.E. 2468–2471 edition;
   - plus the D-tipitaka / E-Tipitaka origin.

### 4.2 Uids and record shape

6. Sutta records:

   | Edition | uid | `language` | `source_uid` |
   |---|---|---|---|
   | Siam Rath Pāli | `<sc_uid>/pli/siam` | `pli` | `siam` |
   | Royal | `<sc_uid>/th/royal` | `th` | `royal` |
   | MCU | `<sc_uid>/th/mcu` | `th` | `mcu` |
   | Mahamakut canon | `<sc_uid>/th/mbu` | `th` | `mbu` |
   | Mahamakut commentary | `<cst_att_text_code>/th/mbu` | `th` | `mbu` |

   - `<sc_uid>` is the SuttaCentral uid of the mūla text (`mn1`, `sn12.1`, `an1.1-10`, `pli-tv-bu-vb-pj1`, `ds1.1`).
   - **`siam` is the edition's source label in uids**, beside `ms` and `cst`. `sya` stays the name of SuttaCentral's reference code for the edition (`sya12.1`), used only in alignment (§4.3) and in the report.
   - Fill `sutta_ref`, `nikaya`, `title` (the Thai heading), `title_ascii`, `title_pali` (from the matching `/pli/ms` record), `content_html` and `content_plain`.
   - **`nikaya` takes the SuttaCentral form** (`mn`, `sn`, `pli-tv-bu-vb`), as the `/pli/ms` records do, not the CST form (`majjhima`). Search's nikāya filter matches it with `LIKE 'prefix%'`.
   - **`title_ascii`** is the Thai title unchanged; ASCII folding has nothing to fold in it.
   - **Fill `sutta_range_group` / `sutta_range_start` / `sutta_range_end` in the bootstrap** (`sutta_range_from_ref()`), so the import never computes them row by row (item 25).
   - Leave `content_json` NULL. A segmented version is later work (§5).
7. **`content_plain` of Siam Rath records stays in Thai script**, so Thai-script search and Thai snippets work (§4.6). Romanization is done when a word is looked up (§4.7), not stored.
8. **Paragraph anchors.** Each Siam Rath paragraph number `[n]` becomes an anchor in `content_html` (`id="siam-<n>"`, Arabic digits). Use the same anchor-jump conventions as the existing texts ([sutta-display-settings-and-multi-column-view.md](../docs/sutta-display-settings-and-multi-column-view.md) §8). The visible number keeps its Thai numerals.
   - **MCU:** `{n}` is the Siam Rath number and gets the anchor. `[n]` is MCU's own number and is shown, without an anchor.
9. **Footnotes.**
   - Siam Rath variant-reading footnotes (at the end of each page) and MCU footnotes (`footer` column) become per-record endnotes. Each reference marker links to its note. The existing footnote rendering applies ([sutta-display-settings-and-multi-column-view.md](../docs/sutta-display-settings-and-multi-column-view.md) §10).
   - MCU's `header` and `display` columns are page chrome and are dropped.
10. **Leave SuttaCentral's Royal texts out.** In `SuttaCentralImporter::import_for_language()` (`bootstrap_cli/src/bootstrap/suttacentral.rs`), skip the `siam_rath` texts when importing `th`. `suttas_lang_th` then keeps only Dhīranando and Jayasāro, and the bundle's Royal records are the only Royal copy.
    - Nothing is lost, because `th` always brings the bundle with it (§4.5).
    - Only `suttas_lang_th.tar.bz2` is changed. The other `suttas_lang_*` archives are built as before.
11. **Mahamakut commentary uids** follow the CST aṭṭhakathā units, so the existing related-text rules find them:
    - **Find the unit.** For a mūla sutta, the unit is the `text_code` of its `att` row in `cst_commentaries` (`mn1` → `mn1.att`, `an1.5` → `an1.1-10.att`, `dhp23` → `dhp21-32.att`, `iti12` → `iti11-13.att`).
    - **Name the record.** The record is `<text_code>/th/mbu`. `related_text_lookup()` takes it as a candidate, "a translation of the commentary from another source" ([sutta-navigation-tree.md](../docs/sutta-navigation-tree.md), "The m | a | t buttons").
    - **The source preference needs one change.** `source_rank` in `related_text_lookup()` (`backend/src/helpers.rs`) ranks a candidate as:

      | Rank | Candidate |
      |---|---|
      | 0 | the same language **and** source as the opened record |
      | 1 | `pli/cst` |
      | 2 | `pli/ms` |
      | 3 | any `pli/` |
      | 4 | anything else |

      So from `mn1/th/royal`, `mn1.att/th/mbu` ranks 4, behind `mn1.att/pli/cst`, and the **a** button opens the Pāli commentary.

      Add a tier between 0 and 1: the candidate's language and source belong to the **same bundle** as the opened record's (`th/royal`, `th/mcu`, `th/mbu` are one family). It reads the same table as removal (item 29). Nothing outside a bundle changes: `mn1/en/sujato` keeps opening `mn1.att/pli/cst`. From `mn1/pli/siam` the Pāli commentary still opens.
    - **Cut the sections.** Split the Mahamakut text at its commentary headings. Seen in the data: `อรรถกถา<title>สูตร`, `อรรถกถาสูตรที่ ๓`, `๒. อรรถกถา<title>ชาดก`, `อรรถกถา<title>สูตรเป็นต้น (๕-๑๐)` for a range, and the closing `จบอรรถกถา<title>`. Assign each section to its suttas by order and by the sutta number or title in the heading.
    - **Merge into units.** Concatenate, in order, the sections that fall in one CST unit into one record.
    - **Book and vagga introductions** (`อรรถกถาสังยุตตนิกาย นิทานวรรค`, `อรรถกถารูปาทิวรรคที่ ๑`) go at the start of the unit of the vagga's first sutta, as CST places them.
    - **A section that covers suttas of several CST units** goes in the unit of its first sutta. The report lists it.
    - **Abhidhamma.** CST has no imported `.att` records for the Abhidhamma, and therefore no `att` rows. Use the uid scheme of §8 Q1, settled during implementation, and record the choice in the feature doc.
    - **Where Mahamakut has no commentary.** The Vinaya volumes (1–10) have no commentary, and Yamaka and Paṭṭhāna appear to have none (no `จบอรรถกถา` markers in volumes 82–91). Confirm this, record it in the report, and create no record for them.
12. **Splitting the Mahamakut canon from its commentary.** In the 91 volumes, the canonical text and the commentary alternate. The canon goes to `<sc_uid>/th/mbu`, the commentary sections to §4.2 item 11. A paragraph that cannot be classified is reported and left out.

### 4.3 Alignment to uids (the import is only as good as this)

13. **Siam Rath Pāli is the hub.** Align it to SuttaCentral uids first. Then align each Thai edition to Siam Rath by the shared paragraph numbers. Royal uses `[n]`, MCU `{n}`, and Mahamakut `[n]` within `volumn_orig`.
14. **Steps for Siam Rath, per volume:**
    1. Parse the joined text into a sequence of headings and paragraphs `[n]`.
    2. Take the SuttaCentral suttas of the volume in order. Their first `sya<vol>.<n>` reference comes from `sc-data/sc_bilara_data/reference/pli/ms/**/*_reference.json`.
    3. Place each sutta at the heading before its `sya` paragraph. Check that the romanized Siam Rath heading matches the sutta's Pāli title, using the same romanizer as §4.7.
    4. When the `sya` reference disagrees with the heading, or is missing, place the sutta by its title and its order between its neighbours. Log the disagreement.
    5. Split suttas that share a paragraph number (SN repetition series, short AN and KN suttas) at their headings inside the paragraph. When the headings cannot be matched one to one, create one range record under the SuttaCentral range uid if there is one, else leave them out. Report it.
15. **Measured before this PRD**, against `thai_royal` with the first `sya` reference of each SuttaCentral sutta:
    - Of 4,640 suttas with a `sya` reference, 4,274 hit a Royal paragraph directly.
    - MCU `{n}` hits 4,192, Mahamakut 4,157, Siam Rath 4,273.
    - 2,641 suttas have no `sya` reference of their own, because they share a paragraph.
    - **The misses are SuttaCentral numbering offsets, not missing text.** For example, SuttaCentral has `mn47` = `sya12.576`, but Siam Rath and Royal have MN 47 (วีมังสกสูตร) at `[๕๓๕]`, in a volume that ends at 566. The same happens in AN 10, DN 23, Bv and Kd.
    - **AN volume 22** (Pañcaka- and Chakkanipāta) has no `sya` references at all and depends on step 14.4.

    These numbers came from a throwaway script, which is not kept. The method is simple enough to rebuild as the regression test of §4.9: for each SuttaCentral `*_reference.json`, take the first `sya<vol>.<n>`, and check that paragraph `n` exists in the edition's volume `vol`. Royal uses `[n]`, MCU `{n}`, and Mahamakut `[n]` grouped by `volumn_orig`. Thai digits are converted.
16. **Overrides.** Hand-made placement decisions go in `bootstrap-assets-resources/thai-tipitaka/alignment-overrides.toml`, keyed by edition, volume and uid. They are our mapping, not a repair of the sources. It follows the pattern of `navigation-blurb-overrides.toml`.
17. **Acceptance per edition and collection.** A collection of an edition (for example MCU / SN) is included only if:
    - every placed record is in uid order;
    - no paragraph is assigned twice;
    - its coverage passes the threshold in the step's config. Start at 95% of the SuttaCentral suttas of that collection; the maintainer sets the final values.

    A collection that fails is left out of the bundle with a warning, and the report says why. Per your answer to Q1, an edition goes in "if it can be well aligned".
18. **The report** is written to `dist/` next to the archive, as `thai-tipitaka-alignment-report.md` with a TSV beside it. It contains:
    - per edition and collection: suttas placed directly by `sya`, by title and order, by override, as ranges, and left out;
    - every `sya` disagreement (uid, SuttaCentral reference, the paragraph found), for the SuttaCentral issue tracker;
    - unclassified Mahamakut paragraphs, and commentary sections spanning several CST units.

### 4.4 Bootstrap step and archive

19. New module `bootstrap_cli/src/bootstrap/thai_tipitaka.rs`. It runs after the SuttaCentral languages step and writes `dist/.../suttas_bundle_thai_tipitaka.sqlite3`:
    - the database has the appdata schema, from `run_migrations`;
    - it holds only the bundle's `suttas` rows.

    It is selected when `--skip-languages` is not given, by `--only-languages th`, and by a new `--only-bundles thai_tipitaka`. `--only-languages th` builds both `suttas_lang_th` and the bundle, since the app always fetches them together.
20. **Indexes.** Build the bundle's fulltext indexes into their own folders, `index/suttas/th.thai_tipitaka/` and `index/suttas/pli.thai_tipitaka/` (§4.6, item 31). Each takes only the bundle's rows of that language.
    - The Thai translations cannot share `index/suttas/th/`, because that folder ships in `suttas_lang_th.tar.bz2`, built from the SuttaCentral Thai texts only.
    - The Siam Rath Pāli cannot share `index/suttas/pli/`, which ships in the general `index.tar.bz2`.
21. **Archive.** Create `suttas_bundle_thai_tipitaka.tar.bz2`, holding the database and both index folders, the way `create_language_archive()` does. Then run `cleanup_language_index_dir()` on both folders. Call `ensure_archivable_db()`, `vacuum_db()` and `analyze_db()` as for the other shipped databases.
22. **Release info: the bundle is described in `languages.json`, which is compiled into the app.**
    - **Where the app's language list comes from.** It is not read from the release at run time. It is `assets/languages.json`, embedded by `include_str!` as `LANGUAGES_JSON` (`backend/src/app_settings.rs`), and read by `get_available_languages()` (`bridges/src/asset_manager.rs`) and by `setup.rs`. The bootstrap copies it into `assets/` on a full run only.
    - **So the bundle goes in the same file.** `write_release_info()` writes the `th` entry with a `bundle` object:
      ```json
      {"code": "th", "name": "ไทย", "sutta_count": 3239,
       "bundle": {"code": "thai_tipitaka", "title": "Thai Tipiṭaka",
                  "archive_bytes": 0, "installed_bytes": 0,
                  "counts": {"siam": 0, "royal": 0, "mcu": 0, "mbu": 0}}}
      ```
      `sutta_count` stays the `suttas_lang_th` count, and the list shows the bundle's counts beside it.
    - **No runtime `bundles.json`.** The checkbox is shown when the compiled entry has a `bundle`, as a language is listed when the compiled list has it.
    - **The parsers must still accept the old entry shape:** `get_available_languages()` keeps its `code|name|count` output and reads the bundle separately; `setup.rs` likewise.
    - `release_info.toml` gets `suttas_bundles = ["thai_tipitaka"]` for the record. `update_checker.rs` parses releases with `suttas_lang` and must ignore the new key, or read it with `#[serde(default)]`; check this with the `releases-fallback.json` tests ([releases-info-and-fallback.md](../docs/releases-info-and-fallback.md)).

### 4.5 Download, import, upgrade, removal

23. **One selection, two controls.** A bundle linked to a language (`"bundle"` in the `languages.json` entry) and that language are the same selection. The rule is one shared QML helper used by both windows, not two copies:
    - **Language → checkbox.** Selecting `th` in the language selector, by typing it or by clicking it in the list, ticks the Thai Tipiṭaka checkbox. Deselecting it unticks the checkbox.
    - **Checkbox → language.** Ticking the checkbox adds `th` to the selector's input and list selection. Unticking removes `th`.
    - **What is downloaded.** The URLs are built from the language selection only. `th` adds both `…/suttas_lang_th.tar.bz2` and `…/suttas_bundle_thai_tipitaka.tar.bz2`. The checkbox state is never a second source of URLs, so the two cannot disagree.
    - **One source of truth.** The selection is the window's `selected_languages`, kept from `language_input.text` by `sync_selection_from_input()`.
      - The checkbox is a **binding**: `checked: root.selected_languages.indexOf(<bundle language>) > -1`.
      - Its `onToggled` only edits `language_input.text`, by adding or removing the code.
      - Do not write `checked` from a `languageSelectionChanged` handler. That signal also fires when code sets the text: `LanguageListSelector.onTextChanged` → `sync_selection_from_input()` → `languageSelectionChanged`. An assignment would also break the binding.
      - `onToggled` fires only on user interaction, so the binding update it causes does not call it back.
    - `validate_language_codes()` is unchanged: `th` is an ordinary code.
24. **Download window** (`DownloadAppdataWindow.qml`, Idx 1). Under "General bundle (always included)", add a CheckBox **"Thai Tipiṭaka"** (the commented-out "Include additional texts" item is the slot for it):
    - description line: "Siam Rath Pāli in Thai script, with the Royal, MCU and Mahamakut Thai translations and the Mahamakut commentaries";
    - it shows its download size from the `bundle` object of the compiled `languages.json` (item 22);
    - unchecked by default, unless `download_languages.txt` pre-selects `th` (item 27);
    - shown only when the compiled `languages.json` describes the bundle;
    - the mobile large-download warning (Idx 2) includes the bundle's size when `th` is selected.
25. **Import.** In `download_urls_and_extract()` (`bridges/src/asset_manager.rs`), a `suttas_bundle_*.tar.bz2` is imported like `suttas_lang_*`:
    - generalize `import_suttas_lang_to_appdata()` in `backend/src/asset_helpers.rs` to also take `suttas_bundle_*.sqlite3`;
    - then `ANALYZE` (already done after a language import; add a row to the table in [user-data-and-sqlite-analyze.md](../docs/user-data-and-sqlite-analyze.md));
    - then `refresh_language_caches()`;
    - then move the index folders into `index/suttas/`.

    The new code must not `canonicalize()` the import path (standing rule). `import_suttas_from_db()` does today; fix it there, since the bundle goes through it.

    **Rewrite the copy in `import_suttas_from_db()`.** Today it loads every row of the import database into one `Vec<Sutta>` and then deletes and inserts row by row, without a transaction. That is fine for a language of a few thousand texts. The bundle has about 30,000 records and about 400 MB of `content_html` + `content_plain`, which would all be in memory at once, more than an Android device can spare. Instead:
    1. `ATTACH` the import database. Use a URI from `normalize_path_for_sqlite()` without `canonicalize()`; a `content://` path is never involved, because the archive was extracted to the app's own temp folder.
    2. Run `DELETE FROM suttas WHERE uid IN (SELECT uid FROM imp.suttas)`.
    3. Run `INSERT INTO suttas (<columns, no id>) SELECT <columns> FROM imp.suttas`.
    4. `COMMIT`, then `DETACH`.

    All four steps run in one transaction. The FTS5 triggers index the rows as they are inserted. The range columns come filled from the bootstrap (item 6). Keep the fallback `sutta_range_from_ref()` only for an older `suttas_lang_*` archive whose range columns are NULL, in a second `UPDATE` pass over those rows. The language archives use the same code, so test it with one of them.
    - **Measured trigram cost:** the dist appdata has 333 MB of `content_plain` and a 39 MB `suttas_fts_data`. The trigram index adds roughly a tenth of the text. The installed size is dominated by the text itself (§7.1).
    - **No searcher reinit.** A language download already ends on a **Quit** screen in both windows, and the new index folders are opened at the next start. The bundle follows the same flow. Calling `reinit_fulltext_searcher()` here would be a new behaviour; the language downloads do not call it today.
26. **`simsapa setup`** (`backend_cli/src/setup.rs`) is the second implementation of this download: change both together.
    - `--languages th` (and the interactive language question) also fetches the bundle, from the same `languages.json` link.
    - No separate bundle flag is needed. The interactive flow mentions the Thai Tipiṭaka when `th` is chosen.
    - Update [backend-cli-and-npm-package.md](../docs/backend-cli-and-npm-package.md) and the self-contained [simsapa-cli-and-localhost-api.md](../docs/simsapa-cli-and-localhost-api.md), which takes no links or source paths.
27. **Upgrade and asset removal.** No new marker file is needed, because `th` implies the bundle.
    - `export_download_languages()` already writes `th` to `download_languages.txt` when Thai records exist. Check that `get_sutta_languages()` sees `th` when only bundle records are present (the user installed the bundle, then removed nothing).
    - The download window and `simsapa setup` pre-select `th` from that file, and with it the bundle (item 23).
    - A user upgrading from a release where `th` had no bundle gets it at the next download. This is intended.
28. **Sutta Languages window** (`SuttaLanguagesWindow.qml`). Above the "Download Languages" selector, add a **Thai Tipiṭaka** checkbox, with the same description and size as in the download window. It is synced with the selector by the shared helper (item 23). "Download Selected Languages" fetches both archives for `th`.
    - **Remove section.** The window also shows a **"Remove Thai Tipiṭaka"** checkbox in the *Remove Languages* section, synced with `th` in the remove selector by the same helper.
      - It has the selector's polarity: ticked means "to be removed", as a selected code there does.
      - It must not be a ticked "installed" box that removes when unticked: in a section where selecting means removing, the opposite polarity would invert the meaning of a tick.
    - Installed means any record with `source_uid IN ('siam','royal','mcu','mbu')`, read by a backend function.
    - The installed list (`get_sutta_language_labels_with_counts()`) shows `th` with its full count. The Siam Rath records are counted under `pli`, which the list does not offer for removal.
29. **Removal.** `remove_sutta_languages()` (`backend/src/db/appdata.rs`) removes a language's linked bundle with it. For `["th"]` it:
    - deletes the `suttas` rows with `language = 'th'`, as today, which covers SuttaCentral Thai and the bundle's Thai records;
    - deletes the bundle's Pāli rows (`language = 'pli' AND source_uid = 'siam'`);
    - appends `th`, `th.thai_tipitaka` and `pli.thai_tipitaka` to `remove_lang_index_dirs.txt`, so the folders are removed at the next start before the searcher opens (`check_remove_lang_index_dirs()`);
    - leaves `pali_edition_twins` alone, which has no `siam` rows (item 46).

    **The marker check rejects these names today.** `check_remove_lang_index_dirs()` (`backend/src/lib.rs`) accepts a line only if every character is ASCII alphanumeric, `-` or `_`, so that a corrupted marker cannot name a path outside `index/suttas/`. A `.` fails it. Extend the check to accept `<code>` or `<code>.<bundle>`, each part non-empty `[A-Za-z0-9_-]+`, which still rules out `..` and `/`. Add a test that `../x` and `th..x` are still refused.

    The link between language and bundle (sources and index folders) is read from one table in the backend, not repeated in QML. Removing `th` must not touch any `/pli/ms` or `/pli/cst` row; test it. The "Remove Thai Tipiṭaka" checkbox and `th` in the remove selector lead to this one removal.
30. Follow the window rules: `onActivated` for any ComboBox, the `Dialog` width clamp and `DialogHeader`, and `AssetManager.set_keep_screen_on("thai_tipitaka_download", …)` around the download and import, released in the completion handler.

### 4.6 Thai fulltext search

31. **One language can have several index folders.** `FulltextSearcher::open_indexes()` (`backend/src/search/searcher.rs`) today treats a folder name as the language key.
    - Accept `<lang>.<bundle>` folder names. The language is the part before the first `.`.
    - Open every folder, and search all folders of a language when the language filter selects it.
    - `register_tokenizers()` and `build_*_schema()` take the language, not the folder name.
    - The same parsing applies everywhere that walks `index/suttas/`. That includes the indexer's per-folder loops (for example `delete_from_dict_index_by_source_uid()`), the Search Index Rebuild window, Storage Diagnostics and the fulltext status count.
    - **Each record is indexed in exactly one folder.** `build_sutta_index()` today indexes every record of a language into `index/suttas/<lang>/`, so an in-app rebuild of `th` or `pli` would put the bundle's records in the base folder as well, and every result would appear twice.
      - A record belongs to the bundle's folder when its `source_uid` is one of the bundle's sources (`siam`, `royal`, `mcu`, `mbu`), and to the language folder otherwise.
      - The rule is in one backend function, read from the same table as removal (item 29), and is used by the bootstrap and the in-app rebuild alike.
      - A rebuild of `th` rebuilds both `th/` and `th.thai_tipitaka/`. A rebuild of `pli` rebuilds `pli/` and, when it exists, `pli.thai_tipitaka/`.
      - **`build_sutta_index(appdata_db, index_dir, lang)` gets this shape.** Today it computes the folder as `index_dir.join(lang)` and selects `WHERE language = lang`. It takes the folder name and a source filter as well, or a target struct that carries them.
      - It keeps computing `has_other_edition` only for the base `pli` folder, as now (`lang == "pli"` must not become true for `pli.thai_tipitaka` by accident).
      - The bootstrap's existing calls keep their meaning.
32. **Thai tokenizer.** Register `th_stem` and `th_normalize` (and the `simple_fold` used for Thai titles) with a **Thai word segmenter** in place of `SimpleTokenizer`.
    - **Candidate:** `icu_segmenter` (ICU4X, Unicode Consortium, widely used, version 2.4).
      - `WordSegmenter::new_auto()` uses its **LSTM** model for Thai, while `new_dictionary()` uses a **dictionary** for Thai (ICU4X docs).
      - Compare both in the planned test. A dictionary needs the `compiled_data` feature only, the LSTM also the `lstm` feature.
    - **Alternatives:**
      - `nlpo3` (PyThaiNLP's newmm in Rust; needs its dictionary file);
      - `charabia`, Meilisearch's tokenizer, which has a Thai segmenter and is used in a search engine.
    - **Pin the segmenter and its data.** The query is segmented at search time with the code in the app, and the index was segmented in the bootstrap. A new data version can split words differently and silently lose hits. So an upgrade of the segmenter crate or its data is treated like a tokenizer change (item 35): it ships with a database release and rebuilt Thai indexes.
    - Tantivy's `QueryParser` turns a query term that segments into several tokens into a phrase query. The `content` fields are indexed `WithFreqsAndPositions` (`schema.rs`), so a phrase works without a schema change.
    - No stemming, but keep `RemoveLongFilter` and lowercasing for any Latin text in Thai records.
    - The choice is made by the planned test in item 34, not before.
33. **Siam Rath Pāli** (`pli.thai_tipitaka`) is space-separated and keeps the `pli` tokenizers.
    - Check that `NiggahitaNormalizer`, `AsciiFoldingFilter` and the Pāli stemmer leave Thai-script tokens intact. Thai `ํ` (niggahita) and `ฺ` (pinthu) must stay in the token.
    - If they do not, give the folder a Thai-script Pāli analyzer without the stemmer.
34. **Planned test, written down here and not run during PRD authoring.** Before choosing the segmenter, index 200 Royal suttas with each candidate and run the query list in the feature doc: common terms, compounds such as `ปฏิจจสมุปบาท`, names, and words that segmentation could split wrongly. Measure:
    - hits and missed hits against Contains Match (FTS5 trigram) as the reference;
    - snippet highlight boundaries;
    - index size and build time;
    - the Android binary size added by the segmenter's data.
35. **The SuttaCentral `th` index uses the same tokenizer.** Rebuilding it happens at the next re-bootstrap. A changed Thai tokenizer means a Thai index built by an older release no longer matches the query analysis, so this ships with a database release (the maintainer sets the version).
36. Contains Match (FTS5 trigram on `content_plain`) already works for Thai and needs no change. Check it with a Thai query in the test list.

### 4.7 Pāli lookups from Thai-script Pāli

37. Add `thai_pali_to_roman()` (backend, pure function) for the Siam Rath orthography:
    - consonants with inherent `a`;
    - `ฺ` (pinthu) as virama;
    - `ํ` as `ṃ`;
    - `ญฺ`/`ญ`, `ฏ`/`ฐ`/`ฑ`/`ฒ`/`ณ` retroflexes, and `ฬ`;
    - vowel signs including the pre-posed `เ` `โ` and `ไ`;
    - Thai digits.

    Test it on the headings of every Siam Rath volume against the matching `/pli/ms` titles (also used in §4.3 step 14.3). Reference implementations: Aksharamukha and the `paliscript` Python library.
38. **Romanize by where the text came from, decided where that is known.** `lookup_from_outside(query)` (`WordSummary.qml`) receives only the text, so the record's language is not known there.
    - The reader page knows its uid. When the selection is sent from a `/pli/siam` page, the page romanizes it before sending, or sends a flag that the receiving QML passes to the backend. Pick one in the tasks, and use it for every path a reader selection takes: word summary, DPD lookup, gloss, Word in Sentence.
    - In the backend, romanization comes **before** `normalize_dpd_query_text()`, and `normalize_dpd_query_split()` passes a query on.
39. **Thai-script Pāli typed in the Dictionary search area.**
    - The romanized query feeds the DPD stream of the three-stream Dictionary page ([search-snippet-highlight-pipeline.md](../docs/search-snippet-highlight-pipeline.md)).
    - The other dictionaries get the query as typed, because a user may have imported a Thai dictionary, which romanization would break.
    - There is no fallback between the two.
40. Text selected in a Thai translation (`/th/` pages) is never romanized.

### 4.8 Reading

41. The translations list (`/translations_for_sutta`) and the column bar offer the bundle's records with the existing labels. Example: "royal (th)", "mbu (th, comm)", "Pāli (siam)". Check `text_kind_label()` and `sort_translations()` with the new sources; `siam` sorts after `ms` and `cst`.
42. `siam` records are Pāli records in another script. Wherever code assumes a Pāli record is in Roman script, exclude `siam` explicitly through one helper (`is_thai_script_pali_uid()`). Known places:
    - `default_english_for_pali` (a `siam` text takes `<base>/th/royal` as its default column);
    - Repeat-Pāli column arrangement;
    - the "any Pāli record" step of `related_text_lookup()`'s source preference;
    - gloss and word selection (romanize, §4.7).
43. `columns_can_align()` treats the bundle's records as their own family. They have no `content_json`, so the multi-column view uses the block fallback for them.

### 4.8b Search options: Siam Rath as a third Pāli edition

44. **Edition checkbox.** In the advanced search options of `SuttaSearchWindow.qml`, beside "MS Mūla in Search" and "CST Mūla in Search", add a **"Siam Rath Mūla in Search"** checkbox. It follows the same pattern as the other two:
    - a stored setting with bridge getter and setter (`get/set_include_siam_mula_in_search_results`), plus the `qmllint` stubs;
    - a filter field `include_siam_mula` beside `include_ms_mula` / `include_cst_mula`;
    - it applies in Fulltext, Contains and Title search: in SQL as `NOT (s.source_uid = 'siam')`, like the `ms` clause in `query_task.rs`, and in Tantivy through the same `source_uid` filter.

    It is shown only when Siam Rath records are installed. It defaults to checked. The localhost API and the CLI build their filters with every edition included (`api/src/helpers.rs`, `cli_support.rs`), so they pass `include_siam_mula: true` and expose no new parameter.
45. **The "Text in both editions" dropdown does not change.** These stay exactly as they are:
    - the dropdown's model (MS / CST / Both);
    - its enabled rule (both the MS and CST checkboxes are checked);
    - `PaliEditionPreference`;
    - `hidden_edition()`;
    - the localhost API and CLI `pali_edition` values (`"ms"`, `"cst"`, `"both"`).

    The dropdown is about MS and CST only. Siam Rath is controlled by its own checkbox (item 44).
46. **A `siam` record is never a twin.** `pali_edition_twins` and the `has_other_edition` index field stay limited to `ms` and `cst`. Write this rule in `docs/thai-tipitaka-bundle.md` and in a comment at `hide_twins_sql()` (`backend/src/pali_edition.rs`), because the reason is not obvious.
    - **How the twin filter works.** It hides **every** twin record of the hidden edition, whether or not the query found its partner: `NOT (uid LIKE '%/pli/<hidden>' AND uid IN (SELECT uid FROM pali_edition_twins))` in SQL, and `has_other_edition` in Tantivy. It is not a comparison of the results.
    - **Why that is safe for MS and CST.** Both are the same Roman Pāli text, so a query that matches one record almost always matches its twin.
    - **Why it would be harmful for Siam Rath.** No query matches both scripts, in Fulltext, Contains or Title search. `suttas_title_match()` matches `title` and `title_ascii`, which are Thai for `siam` records (item 6). So:
      - with preference MS, which would hide the `siam` twins, a Thai-script query, which matches only `siam` records, would return nothing;
      - with a Siam Rath preference, which would hide the `ms` / `cst` twins, a Roman query would return nothing.
    - **Nothing to deduplicate anyway.** A `siam` record and its `ms` / `cst` twin are never found by the same query.
47. **Language filter.** The search language dropdown does not change. `pli` covers the `pli` and `pli.thai_tipitaka` index folders, and `th` covers `th` and `th.thai_tipitaka` (item 31). Contains Match filters on the record's `language`, so both search modes agree.

### 4.9 Tests and documentation

48. **Tests:**
    - importer unit tests on fixture pages: page joining, line-break hyphen joining, `[n]`/`{n}` anchors, footnotes, Mahamakut canon/commentary split, heading patterns;
    - the alignment numbers of §4.3 item 15 as a regression test against the real D-tipitaka files, ignored by default like the full-source tests;
    - `thai_pali_to_roman()`;
    - Thai tokenizer tokens and snippets;
    - `open_indexes()` with `<lang>.<bundle>` folders;
    - removing `th` removes the bundle's Thai and `siam` rows and lists the three index folders, and leaves `/pli/ms` and `/pli/cst` rows;
    - the language selection and the checkbox give the same URL list (a QML test of the shared helper), in both directions and after unticking;
    - the Siam Rath checkbox: unchecked hides `siam` records in Fulltext, Contains and Title search;
    - a Thai-script query finds `siam` records under every `pali_edition` preference (MS, CST, Both), which guards item 46.

    Fixtures written to the dist database use `dummy-` uids and delete exactly what they insert.
49. **Docs:**
    - new `docs/thai-tipitaka-bundle.md` (sources, uids, alignment, report, overrides, tokenizer, romanization, bundle download and removal), with a line in CLAUDE.md's feature list and an entry in `docs/README.md`;
    - updates to PROJECT_MAP.md, `sutta-navigation-tree.md` (Thai commentary records) and `fulltext-index-storage-and-file-locking.md` (`<lang>.<bundle>` folders).

## 5. Non-Goals (Out of Scope)

- **Segmented (`content_json`) Thai or Siam Rath texts**, and line-by-line alignment with `/pli/ms`. This is expected later. The paragraph anchors of item 8 are its starting point.
- **MCU's own Thai commentary translation** (อรรถกถาภาษาไทย ฉบับ มจร., 55 volumes). There is no downloadable source; it is sold in print only.
- **Siam Rath Pāli commentaries** (Hugging Face `uisp/pali-commentary-thai-script-siamrath-version`, 48 volumes). The Pāli commentaries are already in the app through CST.
- **Ṭīkā in Thai.**
- **Navigation-tree entries for the bundle.** The tree keeps opening the edition it opens today.
- **Correcting** D-tipitaka or SuttaCentral data (report only).
- **Showing the existing romanized Pāli in Thai script** (a script-conversion display setting).
- **Other bundles.** The `suttas_bundle_*` mechanism is general, but this PRD ships one.

## 6. Design Considerations

- **Both windows use the same pattern.** The Thai Tipiṭaka checkbox sits above the language selector, and the two are visibly linked: ticking one updates the other at once. There is no separate "bundles" list.
- **Sutta Languages window.** Removal names everything it will delete, for example "Thai (ไทย): SuttaCentral Thai and the Thai Tipiṭaka, including Siam Rath Pāli". It asks for confirmation, as removing a language does.
- **Reader.** Thai records render with the app's Thai-capable font stack. Check that the bundled fonts cover Thai, including `ฺ` and `ํ` on consonants with tall ascenders. The Crimson Pro patch ([crimson-pro-pali-glyph-patch.md](../docs/crimson-pro-pali-glyph-patch.md)) does not cover Thai.

## 7. Technical Considerations

1. **Size.** The raw D-tipitaka text is about 174 MB (Siam Rath 26, Royal 30, MCU 39, Mahamakut 78).
   - **Archive:** expect about 80–150 MB compressed, with indexes.
   - **Installed:** appdata grows by roughly the text twice (`content_html` + `content_plain`), about 350–450 MB, plus about a tenth of `content_plain` for the FTS5 trigram index, plus the two Tantivy folders. For scale, today's dist appdata is 1.1 GB.
   - Measure both in the first bootstrap, put them in the `bundle` object of `languages.json` (`archive_bytes`, `installed_bytes`), and show both in the mobile large-download warning. On a phone the installed size matters more than the download.
2. **Precedent code:**
   - `create_language_archive()`, `cleanup_language_index_dir()` and `write_release_info()` in `bootstrap_cli/src/bootstrap/mod.rs`;
   - `import_suttas_lang_to_appdata()` / `import_suttas_from_db()` in `backend/src/asset_helpers.rs`;
   - `download_urls_and_extract()` in `bridges/src/asset_manager.rs`;
   - `remove_sutta_languages()` and the `remove_lang_index_dirs.txt` marker;
   - `cst_commentaries` and `related_text_lookup()` (`backend/src/helpers.rs`).
3. **The index folder name is the language key** in the searcher today (`open_indexes()`, `open_single_index()`). The `<lang>.<bundle>` naming (item 31) keeps one language per folder for the tokenizer, and lets a bundle ship and remove its own prebuilt index. The other option, re-indexing the language inside the app after import, is too slow on mobile.
4. **MCU numbering.** `{n}` is the Siam Rath paragraph number and `[n]` is MCU's own. Royal and Siam Rath match exactly (volume 12: 566 of 566; volume 15: 957 of 957).
5. **Mahamakut volumes.** `volumn_orig` maps the 91 volumes to the 45. The commentary of each sutta follows the sutta, closed by `จบอรรถกถา…` (for example `จบอรรถกถาโสณทัณฑสูตร ที่ ๔`).
6. **Thai fulltext search is broken today.** `SimpleTokenizer` + `RemoveLongFilter(50)` turns an unspaced Thai phrase into one long token and drops it. So the tokenizer work also fixes the existing SuttaCentral `th` search.
7. **Other datasets** that were checked are copies of D-tipitaka and add nothing:
   - Hugging Face `uisp/tripitaka-siamrath`, which is the Royal translation despite its name;
   - Hugging Face `uisp/tripitaka-mbu`;
   - `visuddhinanda/thai-tipitaka`, a JSONL export with alignment work on Vinaya volume 1 only;
   - `tipitaka-md/siam-rath`, romanized Markdown per volume.

   `visuddhinanda/thai-tipitaka` is worth reading as a cross-check of item 13.

   84000.org is behind a Cloudflare challenge and cannot be fetched by a script.
8. **Bootstrap duration and memory.** Loading the four dumps creates about 550 MB of SQLite. Building three Tantivy indexes is a heavy job. Run it under the memory cap of the global rules.

## 8. Open Questions

1. **Abhidhamma commentary uids.** CST has no imported `.att` records for the Abhidhamma. Should the Mahamakut Abhidhamma commentary use `<sc_uid>.att/th/mbu` per SuttaCentral Abhidhamma uid (`ds1.1.att`), or one record per Mahamakut section? Decide from the Mahamakut section boundaries during implementation.
2. **Edition of the MCU text.** Is D-tipitaka's MCU the B.E. 2539 printing or the revised B.E. 2552 one? Record the answer in the attribution.
3. **Coverage thresholds** per collection (item 17): 95% is a starting value.
4. **Segmenter choice:** `icu_segmenter` or `nlpo3`, decided by the test in item 34, and the Android binary-size budget it may use.
5. ~~`bundles.json` and the release lookup.~~ Settled in review: the bundle is described in the compiled `languages.json` (item 22). The releases info only has to tolerate the new `suttas_bundles` key.
6. **Repeat-Pāli column default for a Thai reader.** On `mn1/th/royal`, should the default Pāli column be `mn1/pli/siam` (Thai script) or `mn1/pli/ms`? The proposal is `siam` when the bundle is installed.
7. **Reporting upstream.** Where do the SuttaCentral `sya` disagreements go: an issue on `suttacentral/sc-data`, or a message to the maintainers? The report format may follow that.
8. ~~Edition dropdown and `siam` twins.~~ Settled in review: Siam Rath gets its own checkbox only. The dropdown, `PaliEditionPreference`, the API's `pali_edition` values and `pali_edition_twins` are unchanged, and a `siam` record is never a twin (items 45–46).
