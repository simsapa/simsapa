# Tasks: Ānandajoti's Jātaka Translation as Sutta Pages (`ja<N>.att/en/anandajoti`)

PRD: `tasks/2026-11-17-095251-prd---jataka-commentary-anandajoti.md`

## Relevant Files

Derived from the finished Dhammapada work (`docs/dhammapada-commentary-anandajoti.md`, `tasks/archive/2026-11-12-134735-tasks-dhammapada-commentary-anandajoti.md` or `tasks/` if not yet moved).

- `bootstrap_cli/src/bootstrap/dhammapada_commentary_anandajoti/` - The model: `epub.rs` (`EpubFiles::from_zip()` / `from_dir()`, `spine()`, `toc()` / `TocEntry`, `sha256_hex()`), `page.rs` (`split_page_marker()`, `substory_label()`, `paragraph_class()`, the span class tables, `render_vagga()`), `extract.rs`, `import.rs`, `mod.rs` (`Manifest`, `ManifestVagga`, `MANIFEST_FILE`, `REPORT_FILE`), `tests.rs`. Its generic parts move to a shared module; the vagga / story structure stays.
- `bootstrap_cli/src/bootstrap/anandajoti_epub/` - **New** shared module (created in 2.0).
- `bootstrap_cli/src/bootstrap/jataka_commentary_anandajoti/` - **New.** `mod.rs`, `extract.rs`, `page.rs`, `import.rs`, `tests.rs`.
- `bootstrap_cli/tests/data/dhammapada-commentary-anandajoti/` - Dhammapada fixture and `expected/`; must pass unchanged after 2.0.
- `bootstrap_cli/tests/data/jataka-commentary-anandajoti/` - **New** fixture (a small unpacked epub, `Fixture-Jataka.epub/`) and `expected/`.
- `bootstrap_cli/src/bootstrap/source_paths.rs` - New dir constant and helper beside `DHAMMAPADA_COMMENTARY_ANANDAJOTI_DIR` (~l. 147); the epub path (`The-Jataka.epub`, `library-imports/books/`).
- `bootstrap_cli/src/bootstrap/mod.rs` - `pub mod` (~l. 12), `pub use` (~l. 54) and the importer call after the Dhammapada commentary block (~l. 352), before the navigation trees.
- `bootstrap_cli/src/bootstrap/preflight.rs` - `bootstrap_sources()`: the manifest as a required source (~l. 128).
- `bootstrap_cli/src/main.rs` - `JatakaCommentaryExtract` subcommand beside `DhammapadaCommentaryExtract` (~l. 1059) and its handler (~l. 1340), and the no-DB match arm.
- `backend/src/helpers.rs` (`split_sutta_ref_kind()`, `related_sutta_uids()`), `backend/tests/test_related_sutta_uids.rs` - Tab matching; the Jātaka uid-set test.
- `backend/src/query_task.rs` (`COMMENTARY_UID_LIKE_PATTERNS`, `commentary_filter_tests`), `backend/src/search/searcher.rs` (`add_sutta_filters()`) - The commentary filters; verify only.
- `src-ts/column_bar.ts`, `src-ts/column_bar.test.ts` - The bottom-bar label (`option_text()`); the test already has `dhp179-196.att/en/anandajoti` → "anandajoti (en, comm)".
- `backend/src/gloss_source.rs`, `backend/tests/test_gloss_source.rs` - Check that a `ja<N>` reference is not affected (the Dhammapada fix was for CST range records).
- `assets/sass/_anandajoti.sass`, `assets/css/suttas.css` - Page styles; the Jātaka wrapper class.
- `backend/tests/data/search_results_pin.tsv` - Regenerated after the bootstrap.
- `docs/jataka-commentary-anandajoti.md` - **New** feature doc. `docs/dhammapada-commentary-anandajoti.md`, `docs/README.md`, `CLAUDE.md` (a symlink to `AGENTS.md`), `PROJECT_MAP.md` - updates.
- `bootstrap-assets-resources/library-imports/library-imports.toml`, `library_imports.rs` - **Untouched** (the `jataka-comm` entry stays).

### Notes

- Run the backend tests with `cd backend && cargo test <name>`, the bootstrap tests with `cd bootstrap_cli && cargo test <name>`, the TypeScript tests with the project's JS test command (`npx jest src-ts/column_bar.test.ts`, or `make test`'s JS step). Run long jobs memory-capped (the user's global instructions). Tell the user before editing any `Cargo.toml` (none is expected: `zip` and `scraper` are already dependencies).
- The backend tests run against the dist `appdata.sqlite3`. Any test that inserts rows uses `dummy-…` uids and deletes exactly those. Never commit `*.sqlite3`. Ask before committing.
- Source defects in the epub are **reported, never repaired**.
- GUI checks are the user's, by hand. Never operate the user's display.
- Write files with `>|` / `>>|` in shell commands (`noclobber`).
- Facts seen while preparing the tasks (confirm them in 3.0): `The-Jataka.epub` has about 140 `Ops/*.html` files and three notes files (`notes.html`, `notes2.html`, `notes3.html`), whose `<a id="aNNNN">` ids are global across the three; `toc.ncx` has one entry per Jātaka, "Ja 62 Aṇḍabhūtajātaka The Birth Story about … (1s)", under "Book I. Ekanipāta …"; the largest content file is `121.html` (143 KB). The dist DB has 547 each of `ja*.att/pli/cst` and `ja*.att/en/opentipitaka`, contiguous 1–547, single numbers (no ranges), with `content_html` NULL (segmented `content_json` only; the largest, `ja542`, is 393 KB), `nikaya` `khuddaka`, `sutta_range_group` `ja`, and `group_path` like "Jātaka-aṭṭhakathā / 1. Ekakanipāto / 7. Itthivaggo". `toc.ncx` has `Ja 1` … `Ja 547` once each, and no `ja*.att/en/anandajoti` rows exist yet. Review findings applied: the page header is modelled on `dhp21-32.att/en/anandajoti`, the CSS selector is extended (no Dhammapada re-bless), and `CopiedColumns` is shared with `dhammapada_tipitaka.rs`.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. This helps track progress and ensures you don't skip any steps.

Example:
- `- [ ] 1.1 Read file` → `- [x] 1.1 Read file` (after completing)

Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

### 1.0 — specs and dependencies

**Depends on:** nothing. Pure tests, no database rebuild.

**Specs:**

- The commentary rule is "the uid's reference ends in `.att` or `.tik`", whatever the source. Nothing here should need a code change; a failing test means a fix in the helper, never in the uids.
- The uid set for one Jātaka: `ja62/pli/ms`, `ja62/pli/cst`, `ja62/en/sujato`, `ja62/en/chalmers`, `ja62/en/opentipitaka`, `ja62.att/pli/cst`, `ja62.att/en/opentipitaka`, `ja62.att/en/anandajoti`, and a neighbour `ja63.att/pli/cst`, `ja63.att/en/anandajoti`, plus `ja6.att/en/anandajoti` (a number that is a prefix of another: `ja62` must not match `ja6`).

- [x] 1.0 Verify what already works for `ja<N>.att/en/anandajoti`: tab matching, the "anandajoti (en, comm)" bottom-bar label, and the commentary search filters (PRD FR 21–24)
  - [x] 1.1 In `backend/tests/test_related_sutta_uids.rs`, add a pure test (no `app_data_setup()`) for the opened uid `ja62/pli/ms` with the uid set in the specs. With `include_cst_commentary = true`, the result contains `ja62.att/pli/cst`, `ja62.att/en/opentipitaka` and `ja62.att/en/anandajoti`, and no `ja63.att/…` or `ja6.att/…`. With `false`, no `.att` uid.
  - [x] 1.2 Same file: for the opened uid `ja62.att/pli/cst`, the result contains `ja62.att/en/anandajoti` and `ja62.att/en/opentipitaka` with both settings, and lists `ja62/pli/ms` (the mūla is always listed). Check also `ja62.att/en/opentipitaka` as the opened uid.
  - [x] 1.3 Same file: `related_sutta_uids_like_pattern("ja62/pli/ms")` is `ja%` (it over-matches by design; `related_sutta_uids()` does the exact match) and, read as SQL `LIKE`, matches each `ja62.att/…` candidate (reuse the `like_matches()` helper added for the Dhammapada tests).
  - [x] 1.3a The m | a | t buttons: add a pure test of `related_text_lookup()` (in `helpers.rs`'s tests or `test_related_sutta_uids.rs`, wherever its Dhammapada cases are) with the uid set in the specs. From `ja62/pli/ms`, the aṭṭhakathā target resolves to `ja62.att/pli/cst` (not to `…/en/anandajoti` or `…/en/opentipitaka`); from `ja62/en/sujato` too; from `ja62.att/en/anandajoti` the mūla resolves to `ja62/pli/ms` or the pre-existing preference, whichever the Dhammapada case shows. A new `…/en/anandajoti` candidate must not change which record a button opens.
  - [x] 1.4 Run `cd backend && cargo test --test test_related_sutta_uids`. If an assertion fails, fix `backend/src/helpers.rs`, not the uids.
  - [x] 1.5 Bottom-bar label: add a case for `ja62.att/en/anandajoti` → `"anandajoti (en, comm)"` to `src-ts/column_bar.test.ts` next to the `dhp179-196.att/en/anandajoti` one (l. 100). Run it. No change to `column_bar.ts` is expected.
  - [x] 1.6 Search filters: add `ja62.att/pli/cst` (`cst`), `ja62.att/en/anandajoti` (`anandajoti`), `ja62/pli/cst` (`cst`), `ja62/en/sujato` (`sujato`) to the doc set of the `DHP_FILTER_DOCS` / `CST_FILTER_DOCS` tests in `searcher.rs` (or a new `JA_FILTER_DOCS`). Assert that `cst_filters(true, false)` drops both `.att` docs and keeps the others. Extend `query_task::commentary_filter_tests` with the same two `ja…att` uids for the Diesel and FTS5 paths.
  - [x] 1.7 Check that `ja<N>` references in `gloss_source.rs` are unaffected (a `ja62` reference reads the same edition as before; the CST-range rule applies to `dhp` only). Add a one-line test only if no existing test covers a `ja` reference.
  - [x] 1.8 Run `cd backend && cargo test searcher`, `cargo test commentary_filter`, `cargo test --test test_search_filter_pagination`, then `cd api && cargo test`. All green, apart from the known timed-test drift.

### 2.0 — specs and dependencies

**Depends on:** nothing in Part 1 (it can be done in parallel with it). A pure refactor: no behaviour change, no database rebuild.

**Specs:**

- Move to a shared module `bootstrap_cli/src/bootstrap/anandajoti_epub/` (split like `epub.rs` and `page.rs`): `EpubFiles` with `from_zip()`, `from_dir()`, `get()`, `spine()`, `ncx_name()`, `toc()`, `TocEntry`, `parse_ncx()`; `sha256_hex()` (now `pub`); `split_page_marker()`; `escape()`; the slug / de-duplicate helper for anchors; the note-ref markup builder and the Notes section builder (if they are separable from the vagga model); the page-frame builder; the `Report` type (defect lines, the summary line, `stdout` + `extract-report.txt` writing).
- Stay in the Dhammapada module: `Vagga` / `Story` / `SubStory` model, `SC_VAGGA_RANGES`, `page_uid()`, `Manifest` / `ManifestVagga`, the Dhammapada class tables (`COMMENTATOR_SPAN_CLASS`, `PLAIN_SPAN_CLASSES`, …), `render_vagga()`, the importer.
- The Dhammapada output must stay byte-identical: the fixture test compares `expected/` byte for byte, and the real-data test (when the epub is present) must give the same 26 pages. `DHP_COMM_BLESS=1` must **not** be used in this task.
- `import.rs`'s `CopiedColumns` (`load()`, `derived()`), `page_plain_text()` and the SHA check are used by the second importer, and `CopiedColumns` is also used by `dhammapada_tipitaka.rs` (the Daw Mya Tin import, `use crate::bootstrap::dhammapada_commentary_anandajoti::import::CopiedColumns`). If they move, update that import in the same change and keep `cargo test dhammapada_tipitaka` green.
- Keep public paths used by `main.rs`, `preflight.rs` (`MANIFEST_FILE`) and `mod.rs` working, or update them in the same change. Docs and code comments that name moved functions are updated in 7.0.

- [x] 2.0 Move the generic epub code of the Dhammapada extractor into a shared module, with the Dhammapada tests unchanged and green (PRD §7, Goal 6)
  - [x] 2.1 (Result: shared `anandajoti_epub/` has `epub.rs` (EpubFiles, TocEntry, `sha256_hex`, `parse_ncx`), `source.rs` (Block, ParsedFile, Notes, `parse_notes`, `find_notes_files`, `text_without_refs`, `is_page_marker`), `page.rs` (`escape`, `split_page_marker`, `Writer` with a `ClassMap` of span / paragraph classes and page-marker classes, `frame()`), `report.rs` (`Report::render`), `import.rs` (`CopiedColumns`, `page_plain_text`, `check_epub_sha256`, `SOURCE_UID`, copyright / licence). The Dhammapada module keeps the vagga / story model, its class tables as `CLASS_MAP`, `render_vagga()` and its importer.) List which functions and types of `epub.rs`, `page.rs` and `extract.rs` are free of Dhammapada concepts (no `Vagga`, `Story`, `SC_VAGGA_RANGES`, `dhp` string). Write the list at the top of this section's results, and decide the module split. Anything that needs a parameter to be shared (e.g. the anchor prefix, the page-frame ids and header) gets one, with the Dhammapada values passed in.
  - [x] 2.2 Run `cd bootstrap_cli && cargo test dhammapada_commentary` first and confirm that it is green before any edit.
  - [x] 2.3 Create `anandajoti_epub/` with `mod.rs`, `epub.rs`, `page.rs`, `report.rs`; register it in `bootstrap/mod.rs`. Move (not copy) the items with `git mv` where whole files move, so the history follows. Re-export from the Dhammapada module only what its own files and `main.rs` still need.
  - [x] 2.4 Move the tests that cover the moved items (epub reading, `split_page_marker()`, anchor slugging, the `sha256` check) to the shared module's tests. The Dhammapada fixture and real-data tests stay where they are.
  - [x] 2.5 Run `cd bootstrap_cli && cargo test dhammapada_commentary`, `cargo test dhammapada_tipitaka` and `cargo test anandajoti_epub`. The fixture output must equal `expected/` unchanged; confirm with `git status` that nothing under `tests/data/` changed.
  - [x] 2.6 If the real epub `Dhamma-Verses-Comm.epub` is present, run the extraction into the scratchpad (not the real output folder) and `diff -r` it against `bootstrap-assets-resources/dhammapada-commentary-anandajoti/`. No difference.
  - [x] 2.7 Run `cd bootstrap_cli && cargo test`, capped as in the Notes. All green except the known timed-test drift.

### 3.0 — specs and dependencies

**Depends on:** 2.0 (to reuse `EpubFiles` to read the epub). Read only: no code beyond a throwaway survey script in the scratchpad.

**Specs:**

- Unpack `bootstrap-assets-resources/library-imports/books/The-Jataka.epub` into its own new, empty directory in the scratchpad (it is untrusted data: do not run anything from inside it). Read only. Scripts live in a different directory and take paths as arguments. Run Python with `-I`.
- The goal is facts, not code: every rule in 4.0 comes from a fact recorded here. Record them in the module doc comment of `jataka_commentary_anandajoti/mod.rs` (created in 4.1) and in this task's results.
- The PRD's Open Questions 1–3 are answered here.

- [x] 3.0 Survey the real Jātaka epub and record the facts (PRD FR 2–5, 9–10; Open Questions 1–3)
  - [x] 3.1 Spine and TOC: read `content.opf` (spine ids, the notes files' place) and `toc.ncx` (nesting depth, `src` anchors). Count the Jātaka entries ("Ja N …"), list the Book / Nipāta entries and the other non-Jātaka entries (Introduction, Studies, Nidānakathā, bibliography, abbreviations, the dates essay). Check the numbers run 1–547 once each, in order; write down any gap, duplicate or reordering (they are defects, reported, FR 14).
  - [x] 3.1a Already checked while reviewing (confirm, do not redo): `toc.ncx` has 547 `Ja N` entries, `Ja 1` … `Ja 547` each once; each title ends in a Nipāta label such as "(1s)" or "(Mahānipāta)" (decide the exact set of labels and how they map to the Nipāta numbers 1–22, and strip them from `title`); the large Jātakas carry nested TOC entries (e.g. Vessantara's "The Small Wood" … "The City"); the epub's content files total 6.7 MB of HTML.
  - [x] 3.2 Headings: for every distinct `<h1>`–`<h6>` class and level, list its role and an example (Book, Jātaka, Pāli title, Story of the Present, Story of the Past, Story of the Connection, verses, word-commentary, …). Find the heading that starts a Jātaka and the one that ends the last Jātaka before the back matter. Find the files where a Jātaka starts or ends mid-file.
  - [x] 3.3 Per-Jātaka structure: take a short one (Ja 1), a medium one, the longest (find it by text size), and a Jātaka with many verses. List the blocks in order: metadata lines, "(1s)"-style section label, Present / Past stories, verse paragraphs, "Word-commentary", the Connection. Does the epub number verses within a Jātaka (OQ 2)? What is the form ("{1.1}", "(1)", "[1]")?
  - [x] 3.4 Classes: for every `<p class>` and `<span class>`, count it, say what it looks like (check `style.css`: italic, bold, colour, gray, superscript, small caps) and what it is for (verse, prose, commentator's note, translator's addition, page marker, note ref). Produce a table for the module doc, as the Dhammapada module doc has. List the classes that do not fit a known role: the extractor must count them as "unmapped markup" in the report.
  - [x] 3.5 Page markers: list the forms (`{…}`, `[189]`, `{J. i. 12}`, a marker split over two spans, a marker inside a verse or heading, a dash after the marker). Compare with `split_page_marker()`'s cases and list the new ones.
  - [x] 3.6 Notes: how many notes in each of the three notes files, the `{N}` numbering (continuous across the files, or restarting?), the `<a id="aNNNN">` ids, the back-link form, and which content files point to which notes file. Find a note that is referenced twice, a note with no reference, and a reference with no note (defects, FR 14). Do notes appear inside headings?
  - [x] 3.7 Links, images, tables: list every `href` that is neither a note nor a same-file anchor (links to another Jātaka, to the introduction, external URLs), every `<img>` and `<table>`. Decide per kind: plain text (FR 8), or something that needs a rule.
  - [x] 3.8 Match the epub's Jātakas to the dist DB: `SELECT uid FROM suttas WHERE uid GLOB 'ja[0-9]*.att/pli/cst'` and the epub's numbers. Report numbers present on one side only (OQ 3). Compare the English titles of a sample of the epub's Jātakas with the `ja<N>.att/en/opentipitaka` titles to settle which title the page carries (the epub's, as the PRD says).
  - [x] 3.9 Sizes: the size of the largest Jātaka's text, and an estimate of the page size with markup. If the largest is far above the Dhammapada's 329 KB (say over 1 MB), write down the consequence for loading on Android (OQ 4) and ask the user whether to split that page (PRD non-goal) before 4.0 starts.
  - [x] 3.10 Decide the anchor scheme (FR 9) from 3.2–3.3: Jātaka `ja-<N>`, section ids (`ja-<N>-present`, `ja-<N>-past`, `ja-<N>-verses`, … or by the epub's own labels, slugged and de-duplicated), verse anchors `ja-<N>-v<k>` only if the epub numbers verses. Decide which headings get a sub-story list (two or more). Record the scheme.
  - [x] 3.11 Write the survey's results into this task (below), and tell the user the answers to Open Questions 1–4 and any decision that needs them (a Jātaka missing from the epub or the DB, an oversized page). Update the PRD's Open Questions with the answers.

**Survey results (3.0).** The epub has 122 content files `001`–`122.html` plus `notes.html`, `notes2.html`, `notes3.html` (spine order, notes last), `toc.ncx` (767 entries, depth ≤ 4).

- **Spine / TOC (3.1):** `toc.ncx` has `Ja 1` … `Ja 547`, each once, in order, all at depth 3. The front matter is `001`–`011` (title page, Introduction, Studies, Bibliography, Abbreviations, Dates essay, Nidānakathā), the Nidānakathā runs to `015`; the Jātakas start in `019.html` (Book I). The epub ends inside Ja 547 (`122.html`): there is no back matter after the last Jātaka. The Nipāta labels in the TOC / headings are unreliable: `(Ns)` ×507, `(Pak)` ×12, `(Mahānipāta)` ×10, none ×16; `(Hound)` and `(King of Mithilā)` are part of English titles. **Rule:** the Nipāta of a Jātaka comes from the last `Book <Roman>.` heading before it (22 Books); the label (`\d+s`, `Pak`, `Mahānipāta`) is stripped from `title` wherever it sits (Ja 377 has it after the Pāli name).
- **Headings (3.2):** `h1.h10` volume ("The Birth Stories, Volume II"); `h2.h20/h21` Book / Nipāta ("Book II. Dukanipāta  The Section with Two Verses", ranges "(464-473)" from Book XII on); `h3.h30/h32` Jātaka ("Ja 62 Aṇḍabhūtajātaka{note}<br/>The Birth Story about … (1s)", exactly one `<br>`: Pāli name before, English title after); `h3.h32` "Preface" (4×, not a Jātaka); `h4.h40–h43` sections inside a Jātaka (99, in 10 Jātakas: 31, 521, 536–539, 543, 545–547); `h5.h51` "End of the First Book". **Rule:** a Jātaka starts at an `h3` matching `Ja N`; any `h1`, `h2`, `h5` or other `h3` ends it (the blocks up to the next Jātaka are skipped: Book intros, Prefaces). 7 Jātakas cross a file boundary (270, 536, 537, 543, 545, 546, 547).
- **Per-Jātaka structure (3.3):** heading, overview (`p5`/`p13`, span `t15`: the translator's summary, identification list, Compare / Keywords lines), Story of the Present (`p5`, `t30`, page markers `t18`), the Cowell verse-translations in the story (`p8`, `t31`), the Pāli verse (`p10`, `t12`, first line "N. …"), Ānandajoti's verse translation (`p19`, `t4`), the Story of the Past and the connection (`p5`, `p20`). No headings divide Present / Past / Word-commentary: only the `h4` sections above. **OQ 2:** verses are numbered per Jātaka, "N. " at the start of the Pāli verse paragraph (`p10`, also `p21`/`p22`); Ja 341 has none; the numbers repeat in 35, 220, 392, 536, 541, 546, have gaps in 27 Jātakas, and Ja 461 does not start at 1: reported, anchors de-duplicated.
- **Classes (3.4):** paragraphs `p4 p5 p7 p3 p20` plain; `p10 p21 p22` verse (Pāli); `p19` verse translation; `p8` quoted verse lines; `p13` hanging-indent list lines; `p14 p15 p23 p24` indented; `p16` stanza spacer (nbsp only). Spans: page markers `t18 t35 t46`; italic `t11 t34 t39 t41 t42 t43 t62 t63 t65`; bold `t14 t33 t47`; bold italic `t25 t60`; superscript `t45 t58 t64 t66`; everything else in the survey is plain (colour / size). A paragraph with a `t15` span is the translator's addition. Anything else is counted as unmapped.
- **Page markers (3.5):** only `[N.N]` and `{N.N}`, alone, as `[N.N] {N.N}`, with spaces or a no-break space around; a span holding only a space also occurs. All are covered by `split_page_marker()`: no new cases.
- **Notes (3.6):** 1766 notes, numbered 1–1766 continuously across the three files (538 / 873 / 355), one `<p class="p13">` each, `<a id="aNNNN"></a><a href="NNN.html#aN"><span>{N}</span></a>` head (the back-link goes to the content file). Every note is referenced exactly once, no ref without a note, no note without a ref, no duplicates. Notes occur inside headings (`{185}` in Ja 1). Note bodies carry spans `t23 t39 t64 t61 t62 t18 t65 t66 t47 t63 t40` and no links.
- **Links, images, tables (3.7):** the only `href`s are note refs (1583 in the Jātaka files); no `<img>`, `<table>` or other tags.
- **DB match (3.8):** 547 contiguous `ja<N>.att/pli/cst` rows match the 547 epub numbers; no `ja*.att/en/anandajoti` rows yet. The epub's English titles differ from OpenTipitaka's ("[62] 2. Commentary on the Aṇḍabhūta Jātaka"): the page carries the epub's title.
- **Sizes (3.9):** total 7.3 MB of epub markup; largest Ja 546 at 295 KB, then 547 (266 KB), 536 (145 KB): no page near 1 MB, so no split question.
- **Anchors (3.10):** `ja-<N>` on the Jātaka's `<section>`; `h4` sections `ja-<N>-<slug>` (slug of the heading's ASCII text, de-duplicated); a section list `nav` when there are two or more `h4` sections; verses `<a id="ja-<N>-v<k>"></a>` before the numbered Pāli verse paragraph; notes `note-ref-<M>` / `note-<M>`.

### 4.0 — specs and dependencies

**Depends on:** 2.0 (shared module) and 3.0 (the rules).

**Specs:**

- **CLI:** `simsapa_bootstrap_cli jataka-commentary-extract [--epub <path>] [--output <dir>]`. Defaults from `source_paths.rs`: the epub at `library-imports/books/The-Jataka.epub`, the output in `jataka-commentary-anandajoti/`. No appdata DB. It exits non-zero only when it cannot read the epub or write the output, never because of a source defect.
- **Output, per Jātaka:** `ja<N>.html`, the whole `content_html` frame: `<div class="suttacentral html-text"><article id='ja<N>.att' lang='en'><header><ul><li class='division'>Minor Collection</li><li class='subdivision'>The Jātaka Commentary</li><li class='nipata'>Book I. Ekanipāta</li></ul><h1>The Birth Story about …</h1></header>…</article></div>`. The header follows the Dhammapada Ānandajoti page's (`dhp21-32.att/en/anandajoti`, built in `page.rs`); the existing `ja<N>.att` CST / OpenTipitaka records have `content_html` NULL (segmented), so there is nothing to copy there. The page body is wrapped `<div class="anandajoti-jataka-comm">` (the Dhammapada pages keep `anandajoti-dhp-comm`).
- **Page body** (names are the contract; element choice is the implementer's, per 3.10): page-level sub-story list when two or more sub-stories; one `<section>` per section; verse paragraphs `<p class="verse">`; `<section class="notes">` last, in order of first reference, the book's numbers kept, a back-link to the first reference.
- **`manifest.json`:** pretty-printed, stable key order:
  `{"epub_file": "The-Jataka.epub", "epub_sha256": "…", "jatakas": [{"uid": "ja62.att/en/anandajoti", "file": "ja62.html", "jataka_number": 62, "title": "The Birth Story about …", "title_pali": "Aṇḍabhūtajātaka", "nipata_number": 1, "nipata": "Ekanipāta", "verse_count": 1}, …]}`.
- **Report:** to stdout and `extract-report.txt`, listing the FR 14 defects, ending in a summary line (Jātakas, notes, defects). No timestamps; sorted maps and book-order vectors (FR 15). `bootstrap_cli` may use `println!`; check how the Dhammapada command prints.

- [ ] 4.0 The `jataka-commentary-extract` command: epub → one HTML page per Jātaka, `manifest.json` and a defect report, with a fixture test and one run on the real epub (PRD FR 1–15)
  - [ ] 4.1 Create `jataka_commentary_anandajoti/` with `mod.rs` (module doc with the 3.x survey results and class table; `Manifest`, `ManifestJataka`, `page_uid(n)`, `MANIFEST_FILE`, `REPORT_FILE`), register it in `bootstrap/mod.rs`. Add `JATAKA_COMMENTARY_ANANDAJOTI_DIR = "jataka-commentary-anandajoti"` with `jataka_commentary_anandajoti_dir()` and the epub path (`The-Jataka.epub`) to `source_paths.rs`.
  - [ ] 4.2 The structure model: `Nipata { number, name }`, `Jataka { number, title, title_pali, sections, verses }`, `Section { label, anchor, nodes }`. Walk the spine as **one stream of block elements**, switch Nipāta / Jātaka / section on each heading (never cut by file). Front matter (before the first Jātaka), the Nidānakathā, and the back matter are skipped (FR 4), recognised by the rules from 3.1–3.2, not by file number.
  - [ ] 4.3 Cross-check each Jātaka heading against its `toc.ncx` entry, and report a mismatch. Parse the number from "Ja N" and the title, with the Pāli name split as 3.2 decided.
  - [ ] 4.4 The note reader: read the three notes files, parse the notes (head `{N}`, text, `<a id>`), map every reference in a page to its note (by the `href` target, not the number alone, in case numbers restart), and keep the book's numbers. Report refs with no note and notes with no ref. Note refs use the `sup.footnote-reference > a` markup (shared builder from 2.0).
  - [ ] 4.5 The page builder: map classes as in the 3.4 table; `<i>`/`<em>` for the translator's additions; `pts-ref` spans for page markers (shared `split_page_marker()`; add the new cases found in 3.5 to the shared function with unit tests, which also run for the Dhammapada); links to other files or Jātakas become plain text; unmapped markup is counted. Keep the text unchanged.
  - [ ] 4.6 Anchors and the sub-story list per the 3.10 scheme, de-duplicated within the page; link text in the lists excludes note refs and page markers (as in the Dhammapada pages, which avoid a second `note-ref-N`).
  - [ ] 4.7 The defect checks of FR 14, each a report line naming the Jātaka and the file: a number missing from 1–547; a number twice; out of order; a Nipāta that differs from the CST row's (the extractor needs no DB: put this one in the 6.2 check, a `sqlite3` query against the manifest, not in the extractor); counts per Nipāta that differ from the epub's own overview (if it has one); refs without notes and notes without refs; a heading that does not match `toc.ncx`; a Jātaka with no body. Never alter the text.
  - [ ] 4.8 Write the outputs: one `ja<N>.html` per Jātaka, `manifest.json` (epub SHA-256 through `sha256_hex()`), `extract-report.txt`, with `std::fs::write` (overwriting), creating the folder if missing.
  - [ ] 4.9 Add `JatakaCommentaryExtract { epub, output }` to `main.rs` (clap, `Option<PathBuf>`, defaults from `source_paths`) and to the no-DB match arm.
  - [ ] 4.10 Build a small fixture in `bootstrap_cli/tests/data/jataka-commentary-anandajoti/Fixture-Jataka.epub/` (an unpacked epub: `content.opf`, `toc.ncx`, two content files, one notes file — or two, if 3.6 shows the notes split matters): one Nipāta with two Jātakas, one crossing the file boundary, one with a Present and a Past story and two verses, two notes (one referenced twice, one in a heading if 3.6 found that), a page marker (also one split over two spans), one link to another Jātaka, and the front matter and Nidānakathā stubs that must be skipped. Commit `expected/` (`ja1.html`, `ja2.html`, `manifest.json`, `extract-report.txt`). `JA_COMM_BLESS=1` re-writes `expected/` (like `DHP_COMM_BLESS`).
  - [ ] 4.11 Tests (in the module): the fixture output equals `expected/` byte for byte; two runs are identical (FR 15); every `href="#…"` has an `id`; no `href` leaves the page; the crossing Jātaka is whole and in order; the sub-story list links to its headings; the notes keep the book's numbers; the front matter and the Nidānakathā make no page; the report names a planted defect (e.g. a missing number in the fixture's gap case).
  - [ ] 4.12 A real-data test, skipped when the epub is absent (pattern: `test_real_data_dhammapada_commentary_extract`): extraction into a temp dir, then assert the Jātaka count from 3.8 (expected 547), numbers 1..=547 unique, the `#`-link check on every page, and the stable manifest. Defects are reported, never a test failure.
  - [ ] 4.13 Run the command on the real epub: `cd bootstrap_cli && cargo run --release -- jataka-commentary-extract` (capped as in the Notes). Read `extract-report.txt`; write its defects and the largest page's size into this task (for the commit message and the doc).
  - [ ] 4.14 Run `cd bootstrap_cli && cargo test jataka_commentary` and `cargo test dhammapada_commentary` and `cargo test anandajoti_epub`. All green.

### 5.0 — specs and dependencies

**Depends on:** 4.0 (the output folder and manifest exist) and the CST import (the `ja<N>.att/en/opentipitaka` rows exist when the importer runs).

**Specs:**

- **Row per manifest entry (FR 16):** `uid` `ja<N>.att/en/anandajoti`; `sutta_ref`, `nikaya`, `group_path`, `group_index`, `order_index`, `sutta_range_*` copied from the `ja<N>.att/en/opentipitaka` row (missing: derive with `SuttaData::parse_range_from_uid()` and warn); `language` `en`, `source_uid` `anandajoti`, `source_language` `pli`; `title`, `title_pali` from the manifest; `content_html` the page; `content_json` NULL; `content_plain` from `sutta_html_to_plain_text()` with the Notes section, note-ref numbers and the `<nav>` lists removed; `copyright` "Ānandajoti Bhikkhu"; `license` "CC BY-SA 4.0"; `source_info` naming *The Birth Stories (Jātakatthavaṇṇanā) (Revised 2023)*, Ānandajoti Bhikkhu, and `https://ancient-buddhist-texts.net/English-Texts/Jataka/`.
- A manifest entry with no `ja<N>.att/pli/cst` row is skipped with a warning (OQ 3), never given a made-up uid.
- **Order in `bootstrap()`:** after the Dhammapada commentary block (`mod.rs` ~l. 352), before the navigation trees.
- **Preflight:** `required("Jātaka commentary (Ānandajoti) pages", …/manifest.json, File)`.
- **SHA check (FR 19):** compare `manifest.epub_sha256` with the SHA-256 of `library-imports/books/<epub_file>`; on a mismatch, or when the epub is missing, `logger::warn("… re-run jataka-commentary-extract")` and continue.
- Reuse the Dhammapada importer's row-building and `content_plain` stripping; if they are generic, move them into the shared module rather than copying them.

- [ ] 5.0 The Jātaka bootstrap importer, preflight source, SHA-256 warning and page CSS (PRD FR 16–20, §6)
  - [ ] 5.1 Write `JatakaCommentaryAnandajotiImporter::new(dir, epub_file)` implementing `SuttaImporter`, reading `manifest.json` (serde structs from 4.1) and failing with a clear error when the folder, the manifest or a page file is missing. Factor the shared parts out of `dhammapada_commentary_anandajoti/import.rs` (row builder, the `content_plain` strip, the SHA check) into the shared module; the Dhammapada importer calls them too, with its test unchanged.
  - [ ] 5.2 Build the rows as in the specs, with the skip-with-warning for a manifest entry that has no CST counterpart, and log the count.
  - [ ] 5.3 Write the SHA-256 check and its warning (FR 19).
  - [ ] 5.4 Call the importer from `bootstrap()` at the position in the specs, with a comment pointing to `docs/jataka-commentary-anandajoti.md` and a `logger::info` line. `pub use` it from `bootstrap/mod.rs`.
  - [ ] 5.5 Add the source to `bootstrap_sources()` in `preflight.rs` as required (the path written once in `source_paths.rs`). Run `cd bootstrap_cli && cargo test test_full_bootstrap_sources -- --ignored`.
  - [ ] 5.6 Test the importer against a scratch appdata (the pattern the Dhammapada importer's test uses): feed it the 4.10 fixture output and stub `ja1.att/en/opentipitaka`, `ja1.att/pli/cst` rows. Assert the uid, the copied columns, `language` / `source_uid`, `content_json` NULL, that `content_plain` holds story text and no note text, and that an entry with no CST row is skipped with a warning. A real-data variant (skipped when the output folder is absent) asserts the row count equals the manifest's.
  - [ ] 5.7 CSS: the rules in `assets/sass/_anandajoti.sass` are scoped to `.anandajoti-dhp-comm` (line 8). Extend the selector to `.anandajoti-dhp-comm, .anandajoti-jataka-comm` and add the Jātaka-specific rules (`.nipata` header item, section headings, anything 3.4 found), with theme-neutral colours. Do not touch the Dhammapada pages' markup or their expected files. Run `make sass`; `assets/css/suttas.css` changes only by the new rules (the tree may have unrelated uncommitted changes there: do not mix them).
  - [ ] 5.8 Check that `library-imports.toml` and `library_imports.rs` are untouched (FR 20), and that `DhammaTextSourcesDialog.qml` lists the Ānandajoti source with a URL; if it names only the Dhammapada commentary, ask the user whether the Jātaka URL should be added (no edit without an answer).
  - [ ] 5.9 Run `cd bootstrap_cli && cargo test`, capped as in the Notes. All green except the known timed-test drift.

### 6.0 — specs and dependencies

**Depends on:** 1.0–5.0. A full bootstrap (hours): follow the heavy-job rules in the Notes.

**Specs:**

- Counts: 547 `ja<N>.att/en/anandajoti`; still 547 each of `ja<N>.att/pli/cst` and `ja<N>.att/en/opentipitaka` (count with `GLOB 'ja[0-9]*'` patterns so `.att` and mūla counts do not mix).
- The Dhammapada counts are unchanged (26 `dhp*.att/en/anandajoti`).

- [ ] 6.0 Re-bootstrap and verify: counts, tabs, the label, search with commentary on and off, search pins, the full tests; hand the GUI checks to the user (PRD FR 21–24, §8)
  - [ ] 6.1 Pre-flight: check for a language server (`ps -eo pid,ppid,etime,rss,args | grep -E "[r]ust-analyzer|[c]langd|…"`, kill it if its parent is `claude`, ask the user if it is the editor's), `free -h`, `df -h .` in `bootstrap-assets-resources/dist`. Tell the user the memory cap and the parallelism and wait for their go-ahead. Run the bootstrap in a `systemd-run --user --scope` with `MemoryMax`, `CARGO_BUILD_JOBS` and `RAYON_NUM_THREADS` set, in the background, the log written with `>|`.
  - [ ] 6.2 Count the rows with `sqlite3` against the dist `appdata.sqlite3` (specs). Spot-check `ja62.att/en/anandajoti`: title, `title_pali`, copied `group_path` / `nikaya`, `content_json` NULL, `content_plain` has story text and no note text, `copyright`, `license`, `source_info` with the URL.
  - [ ] 6.2a Nipāta check: for all 547 manifest entries compare `nipata_number` with the number in `group_path` of the `ja<N>.att/en/opentipitaka` row (`Jātaka-aṭṭhakathā / 1. Ekakanipāto / …`). List the differences in the report notes (they are source or data findings, never fixed here).
  - [ ] 6.3 Translation tabs through the backend (`get_translations_data_json_for_sutta_uid`, or the `simsapa` CLI / API if it exposes it): `ja62/pli/ms` lists `ja62.att/en/anandajoti` with commentary on and not with it off; `ja62.att/pli/cst` lists `ja62.att/en/anandajoti` and `ja62.att/en/opentipitaka` with either setting; the row has `has_content_json` false.
  - [ ] 6.4 Search: the CLI hardcodes commentary on; the *off* state is covered by 1.6 and by checking the `'%.att%/%'` filter on the 547 real rows. Run Contains and Fulltext for a word of one story with commentary on (finds `ja<N>.att/en/anandajoti`), and the SQL filter off (does not). Confirm the Tantivy `en` index counts the 547 new docs.
  - [ ] 6.5 Navigation tree: confirm the CST `ja` leaves open the same uids as before and no defects-report entry is new; the SC spine's `ja` leaves are unchanged.
  - [ ] 6.6 **Regenerate the search pins:** `cd backend && cargo test --test test_search_results_pin -- --ignored --nocapture` (capped; it runs for minutes). Review the `git diff` of `backend/tests/data/search_results_pin.tsv`: totals move for Contains / Fulltext cases that match Jātaka commentary; name any pin that changes for another reason.
  - [ ] 6.7 Run the full tests, capped: `cargo test --no-fail-fast` in `backend`, `bootstrap_cli`, `api`, `backend_cli` (`make rust-test` equivalent), then `make test`'s QML / JS parts. All green apart from the known timed-test drift.
  - [ ] 6.8 Write the GUI checks for the user, to do by hand, in the summary: open `ja62.att/pli/cst` (from the Sutta Navigation), check its tabs and open the Ānandajoti tab; check the bottom bar reads "anandajoti (en, comm)"; use the sub-story links, a footnote pop-up and its back-link; check light and dark themes; turn commentary in translations off and on on `ja62/pli/ms`; open the largest Jātaka (3.9) on Android and check the load time.
  - [ ] 6.9 `git status`: confirm no `*.sqlite3` and no generated tree files are staged by mistake. Do not commit without the user's go-ahead.

### 7.0 — specs and dependencies

**Depends on:** 1.0–6.0 (the docs describe what was built and measured). Docs describe the current behaviour with no history narration; measurements go in the commit message.

- [ ] 7.0 Documentation (PRD FR 25)
  - [ ] 7.1 Write `docs/jataka-commentary-anandajoti.md`: the epub facts from 3.0 (the note files and ref forms included); extract → import and when to re-run the extraction (the SHA warning); the uid scheme shared with the CST `ja<N>.att` records; the anchor scheme; the class mapping; the defect report and the rule that defects are reported, never repaired; what is deliberately left out (front matter, Nidānakathā, back matter, which stay in the library book `jataka-comm`); the `nikaya` copied from the `.att/en/opentipitaka` row; verses with no number in the source having no verse anchor (if so); the shared module and how to add a third Ānandajoti epub.
  - [ ] 7.1a In the doc, state the reach: the pages are in a freshly bootstrapped `appdata.sqlite3`, so users get them with the next appdata release (the version bump is the maintainer's: never run `scripts/set-version.py` here).
  - [ ] 7.2 Add a line under "Feature docs" in `CLAUDE.md` (a symlink to `AGENTS.md`: edit one file only) and an entry in `docs/README.md`.
  - [ ] 7.3 Update `docs/dhammapada-commentary-anandajoti.md`: name the shared module and point to the new doc; fix the names of any moved functions.
  - [ ] 7.4 Update `PROJECT_MAP.md`: the shared module, the new module, the subcommand, the source folder.
  - [ ] 7.5 Check the commentary rule's paragraph in `docs/sutta-display-settings-and-multi-column-view.md` and the new doc agree, and that `docs/` says nothing about "only the Dhammapada" for the `anandajoti` source.
  - [ ] 7.6 Draft the commit message for the user, with the extraction report's defects listed (and the pin diff summary). Ask before committing.
