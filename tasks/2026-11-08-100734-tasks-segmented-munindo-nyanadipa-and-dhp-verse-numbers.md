PRD: [2026-11-08-100734-prd---segmented-munindo-nyanadipa-and-dhp-verse-numbers.md](./2026-11-08-100734-prd---segmented-munindo-nyanadipa-and-dhp-verse-numbers.md)

## Relevant Files

Code (`simsapa/`):

- `bootstrap_cli/src/bootstrap/helpers.rs` - Home of the new shared verse-group range function (`dhp_verse_group_ranges()`), the chapter-title helper, and their unit tests.
- `bootstrap_cli/src/bootstrap/suttacentral.rs` - `bilara_text_to_sutta()` (line ~610) builds every Bilara record from ArangoDB; the Pāli `ms` and Sujato Dhammapada ranges are written here, before `bilara_html_and_plain_text()`.
- `bootstrap_cli/src/bootstrap/suttas_core.rs` - `SuttasCoreDb::build_record()` builds Ṭhānissaro's records; ranges and chapter titles go in here. `SegmentNote` and `footnotes_value_from()` (grouping and encoding of notes) and `write_comments()` are shared with the Nyanadipa importer.
- `bootstrap_cli/src/bootstrap/segment_files.rs` - **New.** Types and readers for `bilara/<sutta_id>.json` and `bilara/footnotes.json`, `hash()` / `text_hash()` / `id_list_hash()` (equal to suttas-core's), `pali_root_ids()` / `pali_template()` / `page_title_key()`, `normalized_source_text()`, the checks behind `check-segment-files` (`check_source_folder()`), and the import shared by both importers: `build_segmented_record()`, `record_title()`, `pali_template_from_db()`, `import_source_folder()`; `test_support` builds a scratch appdata for the importer tests.
- `bootstrap_cli/src/bootstrap/dhammapada_munindo.rs` - Munindo importer: a wrapper over `import_source_folder()` for `dhammapada-munindo/bilara/`, with the real-data test.
- `bootstrap_cli/src/bootstrap/nyanadipa.rs` - Nyanadipa importer: a wrapper over `import_source_folder()` for `nyanadipa-translations/bilara/` and its `footnotes.json`, with the real-data test.
- `bootstrap_cli/src/bootstrap/footnote_sutta_links.rs` - `link_sutta_refs()` / `FootnoteLinker` turn sutta references in notes into `ssp://` links; reused for Nyanadipa's notes.
- `bootstrap_cli/src/bootstrap/mod.rs` - `bootstrap()` (line ~322–345): the Nyanadipa and Munindo steps' `if path.exists()` skips become errors; register the new module.
- `bootstrap_cli/src/bootstrap/preflight.rs` - `bootstrap_sources()` (line ~120): both sources become `required`, pointing at the `bilara/` folders.
- `bootstrap_cli/src/bootstrap/source_paths.rs` - New constants `MUNINDO_BILARA_SUBDIR`, `NYANADIPA_BILARA_SUBDIR`, `SEGMENT_FOOTNOTES_SUBFILE`, and `munindo_bilara_dir()` / `nyanadipa_bilara_dir()` / `nyanadipa_footnotes_file()` for the preflight.
- `bootstrap_cli/src/main.rs` - `Commands` enum (line ~665): new `CheckSegmentFiles` subcommand.
- `bootstrap_cli/Cargo.toml` - Adds `sha2` and `unicode-normalization` (same versions as `backend/`).
- `backend/src/lookup.rs` - `DHP_CHAPTERS_TO_RANGE` (chapter number → verse range); a reverse lookup (range → chapter number) is needed.
- `backend/src/footnotes.rs` - `Footnote`, `encode_footnotes()`, `word_count()`; the stored form of footnotes.
- `bootstrap_cli/tests/data/nyanadipa/snp4.10.html`, `tests/data/dhammapada-munindo/*_expected.html` - Old fixtures of the HTML/Markdown imports; deleted in 6.0.
- `bootstrap_cli/tests/data/segment_files/` - **New.** Fixture source folders (`dhammapada-munindo/`: `dhp1-20`, two verses; `nyanadipa-translations/`: `snp4.2`, three verses and its note) and a truncated `sc-data/` with their Pāli root and template. The tests break copies in memory, one per check.

Data (`bootstrap-assets-resources/`, not in this repo):

- `dhammapada-munindo/markdown/dhp-*.md` - Munindo source text (read-only input).
- `dhammapada-munindo/bilara/dhp*.json` - **New.** 26 segment files.
- `nyanadipa-translations/texts-sc-numbering/snp*.md` - Nyanadipa source text (read-only input).
- `nyanadipa-translations/docs/The_Silent_Sages_of_Old22.pdftotext.txt` - The book: contents page (Sn verse ranges per sutta) and the *Notes* section, for checking.
- `nyanadipa-translations/bilara/snp*.json`, `bilara/footnotes.json` - **New.** 36 segment files and the footnote file (29 notes). `texts-sc-numbering/snp5.18.md` is SC `snp5.19`, so its file is `bilara/snp5.19.json`.
- `sc-data/sc_bilara_data/root/pli/ms/sutta/kn/{dhp,snp}/…` - Pāli root and `html/` template files the segments align to.
- `sc-data/sc_bilara_data/translation/en/sujato/…`, `suttas-core/data/segmentations/than/…` - Reference translations for deciding line placement.
- `suttas-core/src/lib/hash.ts` - The `hash` / `textHash` definitions to match.
- `suttas-core/docs/data-layers.md` - The segmentation-file and footnote rules the new files follow.

- `backend/src/helpers.rs` - `bilara_text_to_segments()` (~2699), `bilara_multi_column_html()` (~2948): where `render_verse_lines()` is applied (7.0).
- `backend/src/app_data.rs` - `default_english_for_pali()` (FR 15a: SuttaCentral's segmented English first) and `sutta_to_segments_json()`, which now always passes the template (verse detection) with `use_template` as the wrap flag.
- `backend/tests/test_default_english.rs` - Default-English tests, including Munindo / Nyanadipa beside Sujato.
- `backend/src/footnotes.rs` - `insert_markers()` / `word_spans()`: markers go in before lines are split.
- `assets/sass/_suttacentral.sass` - `.verse-line` rules; new `.ssp-line` / `.ssp-indent-N`.
- `src-ts/find.test.ts`, `src-ts/pali_word.test.ts` - Find across verse lines, tap-to-lookup at a line start (7.7).
- `assets/css/suttas.css` - Rebuilt by `make sass`.
- `bootstrap_cli/src/bootstrap/suttacentral.rs` - Test that `content_plain` of a segment with `\n` / `\t` has no tab and no joined words (7.6).
- `bootstrap_cli/src/survey_suttas_core.rs` - Calls `build_record()`; output changes with 2.0.
- `docs/simsapa-cli-and-localhost-api.md` - Bundled API doc: `\n` / `\t` in segment text, Munindo / Nyanadipa segmented, Nyanadipa notes in `ssp-fn` form (7.8).
- `docs/sutta-display-settings-and-multi-column-view.md` - Pointer to the line contract (FR 33a).

suttas-core (separate repository, 8.0; branch `than-verse-indent` from `master`, for a PR upstream):

- `suttas-core/src/sources/types.ts` - `ParseOptions { verseIndent }`, passed to `Source.parse()`; off by default.
- `suttas-core/src/sources/than/page.ts`, `index.ts` - Than parser: with `verseIndent`, `p.vN` → N leading tabs, `<pre>` leading tabs kept; tests in `page.test.ts`.
- `suttas-core/src/raw/load.ts` - `loadRaws(translator, options)`.
- `suttas-core/src/raw/patches.ts` - `replace` / `split` / `move` work on a raw with line-start tabs (patches stay written against the plain text); tests in `patches.test.ts`.
- `suttas-core/src/segment/indent.ts` - **New.** Build step: tabs onto stored segment lines, joined verse line breaks restored; tests in `indent.test.ts`.
- `suttas-core/src/segment/footnotes.ts` - `key()` / `normalize()` exported for `indent.ts`.
- `suttas-core/src/build/build.ts` - `--verse-indent` flag, report counts (`indented_lines`, `restored_breaks`, `indent_inside_lines`).
- `suttas-core/scripts/bootstrap/compare-raws.ts` - `--verse-indent` flag.
- `suttas-core/data/sc-data.lock.json` - Re-pinned to `2d9ca8da858` (`sync.ts --write`; no changes to remap).
- `suttas-core/docs/parsers/than.md`, `docs/patching.md`, `docs/data-layers.md`, `AGENTS.md` - The mapping, the option, the patch rule.
- `bootstrap-assets-resources/suttas-core-db/2026-11-12/` - New DB built with `--verse-indent`; `current` repointed (the `2026-10-31` folder is kept).
- `bootstrap_cli/src/bootstrap/suttas_core.rs` - `test_real_data_than_verse_indent_survives_import`.
- `assets/sass/_suttacentral.sass`, `assets/css/suttas.css` - Indent step 1em per level (`vN` is level N, so `v2` = 2em).

Docs and reports:

- `tasks/2026-11-08-100734-review---munindo-nyanadipa-segmentation.md` - **New.** The review report (PRD FR 13).
- `docs/munindo-nyanadipa-segmentation.md` - **New.** Feature doc (PRD FR 30, 33).
- `docs/suttas-core-import.md` - Update §1 position, §3 Ṭhānissaro, §5 markup list (PRD FR 31).
- `docs/text-processing-for-contains-match-and-fulltext-match-search.md` - Update mentions of these sources' HTML (PRD FR 32).
- `CLAUDE.md`, `PROJECT_MAP.md` - Add the new doc and module.

### Notes

- Run tests with `cd bootstrap_cli && cargo test <name>`; `make rust-test` runs all Rust crates. Tests that read the real bootstrap assets skip when the files are absent, as the current importer tests do.
- Tests run against the **dist database**. A test that inserts rows deletes exactly those rows, with `dummy-` uids (PRD FR 29).
- The shell is zsh with `noclobber`: write `>|` / `>>|` for redirections of long test logs.
- **Never sort `content_json` keys**, and never pass Bilara JSON through `serde_json::Value`: use `IndexMap<String, String>` + `serde_json::to_string`.
- Use `try_exists()`, not `.exists()`, in new code.
- No `println!` / `eprintln!` in `backend/`; in `bootstrap_cli` follow the existing style of the file being edited (`logger::*` for the importers).
- Ask before committing; never commit `*.sqlite3`.
- The data files under `bootstrap-assets-resources/` are a separate checkout. Do not commit them from this repo.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. This helps track progress and ensures you don't skip any steps.

Example:
- `- [ ] 1.1 Read file` → `- [x] 1.1 Read file` (after completing)

Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

### 1.0 specs

Depends on: nothing new. Uses `bilara_text_to_sutta()` and `DHP_CHAPTERS_TO_RANGE`.

- **Range function** (in `helpers.rs`):

  ```rust
  /// Heading segment id → verse range ("183–185", "181"), for one Dhammapada
  /// chapter, from its ordered segment ids.
  pub fn dhp_verse_group_ranges(segment_ids: &[&str]) -> IndexMap<String, String>
  ```

  - Works from **segment ids only**. Every Dhammapada template, root and
    translation file of a chapter has the same ids, so the same function
    serves the Pāli, Sujato, Ṭhānissaro and Munindo without reading the Pāli
    text.
  - A heading id is `dhpN:0`, or the last `dhpN:0.x` id of the chapter's first
    verse (`dhp179:0.4`; `0.1`–`0.3` are collection / book / chapter title).
  - A group runs from its heading's verse `N` to the verse before the next
    heading, or to the last verse number present in the chapter.
  - Separator is an en dash (U+2013). One verse → `"181"`.
  - Returns an empty map for a non-`dhp` uid, so callers can call it on every
    record.
- **Writing a range:**
  - Pāli `…/pli/ms`: `"<range>. <existing Pāli text>"` — keep the trailing
    space the Pāli segment already has.
  - Translation: the segment becomes exactly `"<range>."`. If it held
    non-empty text, log a warning (uid, segment id, text) and overwrite
    (PRD FR 22).
  - Apply before `bilara_html_and_plain_text()` so `content_plain` matches
    (PRD FR 23).
- Only `dhp…/pli/ms` and `dhp…/en/sujato` in the SuttaCentral importer. Every
  other language and author is untouched (PRD FR 24).

- [x] 1.0 Dhammapada verse-group ranges: the shared range function, and the ranges written into the Pāli `ms` and Sujato records by the SuttaCentral importer (PRD FR 18–20, 21 Sujato, 22–25, 27)
  - [x] 1.1 Read `bilara_text_to_sutta()` and `get_suttas()` in `suttacentral.rs` to confirm where `json_text` and the uid are both available, and that Pāli `ms` and Sujato Dhammapada records pass through it (not the `html_text` path).
  - [x] 1.2 Add `dhp_verse_group_ranges()` to `helpers.rs` as specified above. Parse ids with a small regex (`^dhp(\d+):(\d+)(?:\.(\d+))?$`); ignore ids that do not match.
  - [x] 1.3 Add `apply_dhp_verse_ranges(content: &mut IndexMap<String, String>, is_pali: bool, uid: &str)` next to it, implementing the "Writing a range" rules, with the warning for non-empty translation headings.
  - [x] 1.4 Unit tests in `helpers.rs` (PRD FR 25), using id lists, not files:
    - [x] 1.4.1 Chapter start: `dhp179:0.1 … 0.4, dhp179:1…, dhp180:1…, dhp181:0` → `dhp179:0.4` = `179–180`.
    - [x] 1.4.2 Single-verse group (`dhp181:0` followed by `dhp182:0`) → `181`.
    - [x] 1.4.3 Last group of a chapter ends at the chapter's last verse (`dhp194:0` … `dhp196:4` → `194–196`, whatever the real data has).
    - [x] 1.4.4 `apply_dhp_verse_ranges`: Pāli prefix keeps the trailing space; translation becomes `"183–185."`; non-`dhp` uid unchanged; key order unchanged.
    - [x] 1.4.5 Real-data test (skips when `sc-data` is absent): for all 26 chapters, read the Pāli root files from `sc-data/sc_bilara_data/root/pli/ms/sutta/kn/dhp/`, compute the ranges, and assert they are contiguous and cover 1–423 with no gap or overlap.
  - [x] 1.5 In `bilara_text_to_sutta()`, for `uid_base` starting with `dhp` and (`lang == "pli" && author == "ms"`) or (`lang == "en" && author == "sujato"`): parse `json_text` into `IndexMap<String, String>`, call `apply_dhp_verse_ranges`, serialise back with `serde_json::to_string`, then continue to `bilara_html_and_plain_text()`. Leave every other record byte-identical.
  - [x] 1.6 Test (PRD FR 27): build a record from a minimal `doc` `Value` (the same shape `bilara_text_to_sutta` receives) for `dhp179-196/pli/ms` with the real root text, and assert `dhp183:0` = `183–185. Ānandattherapañhavatthu ` and the key order equals the input's. Same for a Sujato doc: `dhp183:0` = `183–185.`.
  - [x] 1.7 Test that a non-Dhammapada record (`mn1/pli/ms`) and a Dhammapada record of another author (`dhp179-196/de/sabbamitta`) come out unchanged.
  - [x] 1.8 `cd bootstrap_cli && cargo test` passes (apart from known timing drift).

### 2.0 specs

Depends on: 1.0 (`dhp_verse_group_ranges`, `apply_dhp_verse_ranges`).

- In `SuttasCoreDb::build_record()`, after `content` is built and before
  `content_json` is serialised, for `translator == "than"` and a `dhp`
  `sutta_id`:
  - `apply_dhp_verse_ranges(&mut content, false, …)`.
  - Chapter title (PRD FR 24a–24b): the template key whose template value
    contains `class='range-title'` (that is `dhpN:0.3` today; find it by the
    template rather than hard-coding `0.3`). Value `"<chapter>. <title>"` for
    **all 26**. Title = the segment's existing text trimmed (22 chapters have
    one, e.g. `"Happy\n\n"` → `Happy`), else `self.title(…)` (the 4 empty
    ones: dhp21-32, dhp76-89, dhp179-196, dhp273-289). If both exist and
    differ, log both and use the segment's text. Both empty → warning, segment
    left empty. Chapter number from a reverse lookup of
    `DHP_CHAPTERS_TO_RANGE` by the `sutta_id` range.
  - The heading `dhp239:0` holds `235-238*` (a suttas-core defect). The range
    overwrites it with the FR 22 warning; 4.1 records it in the review report
    as an upstream defect.
  - `survey_suttas_core.rs` also calls `build_record()`; its output will now
    show the ranges and titles. Check its comparison still reads sensibly.
- `content` is `IndexMap<&str, String>` there; adapt the helper's signature
  (generic over the key type, or build an owned map) without changing key order.
- The shared title helper is reused by the Munindo importer in 6.0:

  ```rust
  pub fn dhp_chapter_number(sutta_id: &str) -> Option<u32>   // "dhp179-196" → 14
  pub fn range_title_key(template: &IndexMap<String, String>) -> Option<&str>
  ```

- [x] 2.0 Ṭhānissaro's Dhammapada in `SuttasCoreImporter`: verse-group ranges in the heading segments and `<chapter>. <title>` in the chapter-title segment (PRD FR 21 Ṭhānissaro, 24a–24b, 27a part)
  - [x] 2.1 Add `dhp_chapter_number()` (reverse of `DHP_CHAPTERS_TO_RANGE`) and `range_title_key()` to `helpers.rs`, with unit tests (`dhp1-20` → 1, `dhp383-423` → 26, `mn1` → `None`).
  - [x] 2.2 Make `apply_dhp_verse_ranges` usable on the `IndexMap<&str, String>` in `build_record()` (generic key or a small adapter), keeping order.
  - [x] 2.3 In `build_record()`, apply the ranges and the chapter title for `than` Dhammapada texts, as in the specs, before `serde_json::to_string(&content)`.
  - [x] 2.4 Tests in `suttas_core.rs`, using the existing in-memory suttas-core fixture pattern (`from_connection`, the `CREATE TABLE sutta_segments …` test setup): a `dhp179-196` `than` text with an empty `0.3` gets `14. Awakened` from the title; a `dhp197-208` text with `0.3` = `"Happy\n\n"` gets `15. Happy`; `dhp183:0` = `183–185.`; a non-empty heading (`235-238*`) is overwritten with a warning; an `mn` text is unchanged.
  - [x] 2.5 Real-data test (skips when the suttas-core DB is absent): all 26 `than` Dhammapada records built by `build_record()` have a non-empty range-title segment and a range in every heading segment.
  - [x] 2.6 Run `survey-suttas-core` on a few Dhammapada chapters and confirm its report is not thrown off by the added ranges and titles; adjust its comparison if it is.
  - [x] 2.7 `cd bootstrap_cli && cargo test` passes.

### 3.0 specs

Depends on: 1.0 (ranges are *not* in the files — headings are `""`, FR 8). Independent of 2.0.

- **Segment file** `bilara/<sutta_id>.json` (PRD FR 2):

  ```rust
  #[derive(Deserialize, Serialize)]
  pub struct SegmentFile {
      pub sutta_id: String,
      pub translator: String,          // "munindo" | "nyanadipa"
      pub input: String,               // "markdown/dhp-14.md", relative to the source folder
      pub input_hash: String,          // text_hash(source text)
      pub pali_hash: String,           // id_list_hash(Pāli ids)
      #[serde(default)]
      pub introduction: Option<String>,// HTML; FR 11, empty today
      pub segments: Vec<Segment>,      // order = Pāli order
  }
  #[derive(Deserialize, Serialize)]
  pub struct Segment { pub id: String, pub text: String, pub source: String } // "copy" | "manual"
  ```

  Unknown `source` values are an error (suttas-core's other values —
  `llm`, `offsets`, … — never occur here).
- **Footnote file** `bilara/footnotes.json` (PRD FR 10):

  ```rust
  pub struct FootnoteFile { pub footnotes: Vec<FootnoteEntry> }
  pub struct FootnoteEntry {
      pub sutta_id: String, pub segment_id: String, pub marker: String,
      pub number: u32, pub word_index: Option<u32>, pub content: String, pub editor: bool,
  }
  ```

- **Hashes** must equal suttas-core's (`suttas-core/src/lib/hash.ts`):
  - `hash(t)` = first 16 hex chars of SHA-256 of NFC(`t`).
  - `text_hash(t)` = `hash` of `t` with `\[\^[^\]]+\]` and `\[\]` removed,
    `\s+` → `" "`, trimmed.
  - `id_list_hash(ids)` = `hash(ids.join("\n"))`.
- **Source-text normalisation** used by the "joined text" check (PRD FR 4,
  14) — one function, `normalized_source_text(source_md, kind)`, documented
  in the new doc:
  - remove verse-number lines/prefixes (Munindo `^\d+(-\d+)?\.\\\\$`,
    Nyanadipa `^\d+\\\. `), the `#` / `===` title line, footnote markers and
    the `[^n]:` definitions;
  - remove Markdown line-break syntax (`\\` at line end, two trailing spaces);
  - strip Markdown emphasis to plain text for comparison (`*x*` → `x`) and
    HTML tags from segment text (`<i>x</i>` → `x`);
  - compare after `\s+` → `" "` on both sides.
  - The chapter / sutta title is compared separately against the range-title /
    sutta-title segment.
- **Check command:**
  `simsapa_bootstrap_cli check-segment-files <source_folder> [--sc-data <dir>]`
  — default `--sc-data` from `source_paths`. For every `bilara/*.json`:
  1. `segments` ids == Pāli root ids, same order (report first divergence);
  2. joined segment text == normalised source text (report the first
     differing word with ±40 chars of context, and the segment id);
  3. `input_hash` / `pali_hash` current (report "stale");
  4. heading segments (`dhpN:0`, the first verse's last `0.x`) are `""`;
     the title segment equals the source title (with the `14. ` prefix for
     Munindo);
  5. footnotes: every `[^n]` of the source appears exactly once in
     `footnotes.json`, on a segment of that sutta, and `word_index` is
     `< word_count(segment text)`. Use `backend/src/footnotes.rs`
     `word_count()` itself, which counts words of the stored HTML text
     (`word_spans()`, split on `char::is_whitespace()`), so the check and the
     renderer can never disagree.
  Prints every problem, a summary line, exits 1 on any problem. Read-only.

- [x] 3.0 Segment-file support in `bootstrap_cli`: the file and footnote-file types and readers, suttas-core-compatible `textHash` / `pali_hash`, and the `check-segment-files` command (PRD FR 1–3, 10 format, 14)
  - [x] 3.1 Add `MUNINDO_BILARA_SUBDIR = "bilara"`, `NYANADIPA_BILARA_SUBDIR = "bilara"`, `SEGMENT_FOOTNOTES_SUBFILE = "bilara/footnotes.json"` (and any path helper) to `source_paths.rs`.
  - [x] 3.2 Create `bootstrap_cli/src/bootstrap/segment_files.rs` with the `SegmentFile`, `Segment`, `FootnoteFile`, `FootnoteEntry` types and `read_segment_file(path)`, `read_segment_dir(dir) -> Vec<(PathBuf, SegmentFile)>` (sorted by file name), `read_footnote_file(path) -> Option<FootnoteFile>` (absent file = `None`). Register the module in `bootstrap/mod.rs`.
  - [x] 3.3 Add `hash()`, `text_hash()`, `id_list_hash()`; add `sha2` / `unicode-normalization` to `bootstrap_cli/Cargo.toml` if not already reachable.
  - [x] 3.4 Test the hashes against suttas-core's own values: take `input_hash` and `pali_hash` from a real `suttas-core/data/segmentations/than/*.json` and recompute `pali_hash` from that file's ids (skip when absent). For `text_hash`, compute the expected value once with `bun -e` from `suttas-core/src/lib/hash.ts` on a short string with a `[^1]` marker and multi-line whitespace, and hard-code it in the test.
  - [x] 3.5 Add `pali_root_ids(sc_data_dir, sutta_id) -> Vec<String>` (ordered, read with `IndexMap` from the `root/pli/ms` file, found by walking `root/pli/ms/sutta/kn/{dhp,snp}`) and `pali_template(sc_data_dir, sutta_id)` for the `html/` file.
  - [x] 3.6 Add `normalized_source_text()` and `joined_segment_text()` per the specs, with unit tests on short Munindo- and Nyanadipa-style snippets (verse numbers, `\\`, two-space breaks, `*italics*`, `[^1]`, footnote definitions).
  - [x] 3.7 Implement the five checks as functions returning `Vec<Problem>` (`Problem { file, segment_id: Option<String>, message }`), so tests can call them without the CLI.
  - [x] 3.8 Add the `CheckSegmentFiles { source_folder, sc_data }` subcommand to `main.rs`, printing problems and the summary, exit code 1 on any problem.
  - [x] 3.9 Fixtures in `bootstrap_cli/tests/data/segment_files/` (a two-verse fake source, a matching segment file, a broken copy for each check) and tests that each check reports its problem and the good file reports none.
  - [x] 3.9a Check 6, coverage (`check_coverage()`): every source text (`markdown/dhp-*.md`, `texts-sc-numbering/*.md`; `SourceKind::source_texts()`) is the `input` of exactly one segment file and every `input` is a source text, so with check 2 the files join up to the whole original. Straight apostrophes after a letter compare as `’` (`curly_apostrophes()`). Tests added.
  - [x] 3.10 `cd bootstrap_cli && cargo test` passes; `cargo run -- check-segment-files <munindo folder>` runs and reports that there are no files yet (not a crash).

### 4.0 specs

Depends on: 3.0 (check command). This task is **data work**, in
`bootstrap-assets-resources/dhammapada-munindo/bilara/`.

- One file per chapter, `dhp<start>-<end>.json`, ids from the Pāli root.
- Placement rules (PRD FR 4–8): author's lines, author's order; every line in
  exactly one segment; `\n` between lines inside a segment; trailing space
  kept at segment ends; meaning decides the Pāli verse, **never Munindo's
  numbers**; a verse spanning Pāli verses splits at the point the meaning
  passes over, like Sujato / Ṭhānissaro at dhp186–187.
- Title: `dhpS:0.3` = `"<chapter>. <Munindo title>"`; `0.1`, `0.2`, all
  heading segments `""`.
- `source`: `copy` for empty/structural segments, `manual` for placed text.
- Review report entries (PRD FR 13), one per case: file, segment ids, what
  the source has, what the Pāli has, what was done. At least: every place
  Munindo's number disagrees with the placement, every verse split that could
  not fall on a sentence boundary, every Pāli verse with no translation.

- [x] 4.0 Write and verify the 26 Munindo segment files (`dhammapada-munindo/bilara/`), with the review report entries; the check command reports no problems (PRD FR 4–8, 11–13)
  - [x] 4.1 Create the review report `tasks/2026-11-08-100734-review---munindo-nyanadipa-segmentation.md` with a header, the entry format, a Munindo section, and an "Upstream defects (suttas-core)" section starting with Ṭhānissaro's `dhp239:0` = `235-238*`.
  - [x] 4.2 For each chapter `dhp-N.md`, confirm it is the chapter `DHP_CHAPTERS_TO_RANGE[N]` (title ↔ Pāli vagga, first and last verse ↔ first and last Pāli verse of the range) before writing anything (PRD FR 12). Record any mismatch.
  - [x] 4.3 Write a small generator, kept out of the repo in the scratchpad, that produces a **skeleton** file per chapter: every Pāli id with `""`, the title segment filled, the hashes computed. The placement itself is done by reading, not by the generator.
  - [x] 4.4 Place the lines, chapter by chapter, comparing the Pāli, Sujato's and Ṭhānissaro's segments for each verse. Run `check-segment-files` after each chapter and fix every reported problem before moving on.
  - [x] 4.5 Record each case in the review report as you go (see specs).
  - [x] 4.6 Spot-check five chapters in full by reading the Pāli and the placed lines side by side (e.g. chapters 1, 5, 14, 18, 26), including every multi-verse Munindo verse in them.
  - [x] 4.7 `check-segment-files dhammapada-munindo` reports zero problems for all 26 files.

### 5.0 specs

Depends on: 3.0. Same rules as 4.0, in `nyanadipa-translations/bilara/`.
Additions:

- Title: `snpX.Y:0.2` = the Markdown `#` heading, short form (`The Cave`).
  It must name the same sutta as the Pāli title (`Guhaṭṭhakasutta`); confirm
  with the book's contents page, which pairs the long English title with its
  Sn number.
- Prose segments with no translation stay `""` (PRD FR 9).
- Check first, and explain in the report: `snp1.12` (15 verses / 17 Pāli
  groups), `snp3.6` (38 / 54 — the book's range 510–547 is 38 verses, so
  likely the prose frame), `snp4.16` (21 / 23), `snp5.18` (19 / 8).
- Nyanadipa's own `texts/` numbering is shifted in the Pārāyanavagga; use
  only `texts-sc-numbering/`, and confirm each file's content is the SC sutta
  of that number.
- Footnotes: one `footnotes.json` for all 36 suttas; every `[^n]` of the
  Markdown → one entry. `marker` = the Markdown marker, `number` = 1-based
  order within the sutta, `segment_id` = segment holding the marker,
  `word_index` = 0-based index (in that segment's stored text, counted the way
  `backend/src/footnotes.rs` `word_count()` counts) of the word the marker
  follows, `null` only for a whole-segment note; `content` = the note's
  Markdown converted to HTML (italics → `<i>`), wording unchanged;
  `editor: false`. The book's *Notes* section (pdftotext, p. 175) is the
  cross-check that none is missing.

- [x] 5.0 Write and verify the 36 Nyanadipa segment files and `footnotes.json` (`nyanadipa-translations/bilara/`), with the review report entries; the check command reports no problems (PRD FR 4–6, 8–13)
  - [x] 5.1 Add a Nyanadipa section to the review report.
  - [x] 5.2 For each of the 36 files, confirm title, verse count and content match the Pāli sutta of its SC number, using the contents page as the second check (PRD FR 12). Examine the four flagged suttas first and record the explanation for each.
  - [x] 5.3 Generate skeletons as in 4.3 (title in `0.2`).
  - [x] 5.4 Place the lines sutta by sutta (Pāli, Sujato and, for the Aṭṭhakavagga and Pārāyanavagga, Ṭhānissaro's segments as references), running the check after each sutta.
  - [x] 5.5 Build `footnotes.json`, removing each marker from the segment text and recording its segment and word index. Compare the notes with the book's *Notes* section.
  - [x] 5.6 Record every case in the report.
  - [x] 5.7 Spot-check five suttas in full, including `snp3.6` and `snp5.18`, and every footnote position in them.
  - [x] 5.8 `check-segment-files nyanadipa-translations` reports zero problems for all 36 files and the footnote file.

### 6.0 specs

Depends on: 1.0 (ranges), 2.0 (`dhp_chapter_number`, `range_title_key`), 3.0 (readers), 4.0 and 5.0 (data).

- Both importers, per segment file (PRD FR 15):
  1. Template = `content_json_tmpl` of `<sutta_id>/pli/ms` from the appdata DB
     (the SuttaCentral step has run). Missing → error naming the uid.
  2. `content: IndexMap<String, String>` = one key per template key, in
     template order, value from the file or `""`; a file id not in the
     template is an error (the check command should have caught it).
     `consistent_niggahita()` per value.
  3. Munindo only: `apply_dhp_verse_ranges(&mut content, false, uid)`.
  4. `content_json` / `content_json_tmpl` via `serde_json::to_string`;
     `content_plain` via `bilara_html_and_plain_text()`, with
     `introduction`'s plain text prepended when present.
  5. `content_html = NULL`, `description = introduction`.
  6. Footnotes (Nyanadipa): entries of this `sutta_id` → `Footnote`s with
     `html = link_fn(consistent_niggahita(content))` → the same
     `IndexMap` segment-id → `encode_footnotes()` value as
     `footnotes_value()` → `write_comments()` after the insert. Move the
     shared part out of `SuttasCoreDb::footnotes_value()` into a free function
     (`footnotes_value_from(notes_by_segment, template)`) so both call it.
  7. `uid`, `title` (record title = source title, unchanged: `The Buddha`,
     `The Cave`), `sutta_ref`, `nikaya`, `source_uid`, range fields as today.
- Uids unchanged: `dhp179-196/en/munindo`, `snp4.2/en/nyanadipa`.
  One exception: the song of Piṅgiya is SC `snp5.19` (review report), so
  its record becomes `snp5.19/en/nyanadipa`, no longer `snp5.18/en/nyanadipa`.
  Derive the uid from the file's `sutta_id`, never from the `input` file name.
- Link resolution for notes: build a `SuttaRefResolver`
  (`footnote_sutta_links.rs`) from the Pāli `ms` uids in the DB, with an empty
  Ṭhānissaro mapping slice, and pass
  `|html| link_sutta_refs(html, &|r| resolver.resolve_pli(r))` as `link_fn`,
  the way `SuttasCoreImporter` builds its own. `Sn 4:17` and `Sn 204` do not
  resolve and stay text.
- `source_info` = `"dhammapada-munindo/bilara"` / `"nyanadipa-translations/bilara"`.
- **Default English (PRD FR 15a):** in `default_english_for_pali()`
  (`backend/src/app_data.rs`), the preference becomes: segmented with
  `source_info` NULL (SuttaCentral import) → any segmented → any; AI English
  still excluded and `/pli/cst` unchanged. Must land in the same change as
  6.3–6.4, or Munindo / Nyanadipa replace Sujato as the default.

- [x] 6.0 Switch the Munindo and Nyanadipa importers to the segment files: `content_json` / template / plain text / footnotes / titles / ranges, remove the HTML/Markdown paths, make both sources required in the preflight and in `bootstrap()`, keep Sujato the default English (PRD FR 15–17, 15a, 21 Munindo, 26, 27a part, 28, 29)
  - [x] 6.1 Refactor `SuttasCoreDb::footnotes_value()` so the grouping-and-encoding part is a free function used by both; `suttas_core.rs` tests still pass unchanged.
  - [x] 6.2 Add a shared `build_segmented_record(file, template, …)` in `segment_files.rs` doing steps 2–5 of the specs, returning the fields the importers insert. Unit-test it with the 3.9 fixtures and a fixture template (key order, empty keys, niggahita, introduction in `content_plain`).
  - [x] 6.3 Rewrite `DhammapadaMunindoImporter`: read `bilara/` via `read_segment_dir`, derive the chapter / uid from `sutta_id`, build and insert each record, apply the ranges. Remove `discover_html_files` and the HTML `parse_sutta`.
  - [x] 6.4 Rewrite `NyanadipaImporter`: read `bilara/` and `bilara/footnotes.json`, build and insert each record, write its comments. Remove the Markdown parsing and the `mdbook` call; drop the `mdbook` dependency from `bootstrap_cli/Cargo.toml` if nothing else uses it. (Kept: `buddha_ujja.rs` still calls `mdbook::utils::render_markdown`.)
  - [x] 6.4a Record `title` (both importers): the importers no longer read the Markdown, so derive it from the page-title segment (`page_title_key()`): Nyanadipa the segment's text, Munindo the text with its `<n>. ` prefix removed. The 16 title segments whose Markdown heading has a straight `'` already hold `’` (decided: the book and today's mdbook-made titles are curly; the check applies `curly_apostrophes()`), so the titles come out as today. Test that the 62 record titles equal today's, `snp5.19` taking the old `snp5.18` record's title (`Upasīva’S Question` keeps its capital `S`, a recorded source defect).
  - [x] 6.5 Both importers: a missing `bilara/` folder or an empty one is an error; one file failing to import is an error (no silent per-file skip), so a release cannot ship with a chapter missing.
  - [x] 6.6 `preflight.rs`: change both entries to `required(…)` with the `bilara/` paths (and the footnote file for Nyanadipa) from `source_paths`.
  - [x] 6.7 `mod.rs` `bootstrap()`: replace the two `if path.exists() … else warn` blocks with unconditional imports that propagate errors.
  - [x] 6.8 Replace the old tests (`test_parse_dhp1`, `test_snp4_10`, the HTML-regex tests) and delete `tests/data/nyanadipa/snp4.10.html` (PRD FR 28). Also deleted the unreferenced `tests/data/dhammapada-munindo/*_expected.html`.
  - [x] 6.9 New importer tests against the real assets (skip when absent), each inserting under `dummy-` uids or in a transaction rolled back at the end, and cleaning up only what it wrote (PRD FR 26, 29). Done in a scratch appdata (temp dir, real migrations) seeded with the `dhp`/`snp` `pli/ms` templates read from the dist DB opened read-only, so nothing is written to the dist DB (`segment_files::test_support`).
    - [x] 6.9.1 `dhp179-196` Munindo: `content_json` keys == Pāli template keys in order; `dhp183:0` = `183–185.`; `dhp179:0.3` = `14. The Buddha`; `content_plain` contains *Refrain from doing evil*; `content_html` is NULL.
    - [x] 6.9.2 `snp4.2` Nyanadipa: `snp4.2:0.2` = `The Cave`; one `sutta_comments` row whose value parses (via `backend/src/footnotes.rs`) to note 1 on the expected segment; *Incorrigible* is not in `content_plain`. (The verse itself has *incorrigible*, the word the note glosses, so the test checks the note-only phrase *the root meaning*; `content_plain` is lowercased search text.)
  - [x] 6.9.3 Default English (PRD FR 15a): change `default_english_for_pali()` as in the specs, update its doc comment and `docs/suttas-core-import.md` "The default English beside a Pāli text"; tests in `backend/` that `dhp1-20/pli/ms` and `snp4.2/pli/ms` default to `…/en/sujato` with Munindo / Nyanadipa segmented, and that a Pāli text whose only segmented English is from suttas-core still gets it.
  - [x] 6.10 Run the preflight test `cd bootstrap_cli && cargo test test_full_bootstrap_sources -- --ignored` against the real checkout.
  - [x] 6.11 `make rust-test` passes.

### 7.0 specs

Depends on: nothing in 1.0–6.0 (can be done in parallel). PRD FR 34–40, 33a–33c.

- **Shared function** in `backend/src/helpers.rs`:

  ```rust
  /// Renders a verse segment's line breaks and indent levels. `template_entry`
  /// decides whether the segment is verse (`class='verse-line'`). Returns the
  /// input unchanged when the segment has no `\n` inside it and no leading tab.
  pub fn render_verse_lines(segment_html: &str, template_entry: &str) -> Cow<'_, str>
  ```

  - Split on `\n`; drop a final empty piece (trailing `\n`); keep the
    segment's trailing spaces after the last span.
    - Each line: count and strip leading `\t` → level N; wrap as
      `<span class='ssp-line ssp-indent-N'>…</span>` (N = 0 gets
      `ssp-indent-0`, so the CSS is uniform).
  - Called **after** footnote markers are inserted (`insert_markers`) and
    after any other per-segment processing (variants, glosses), on both paths:
    `bilara_text_to_segments()` and the per-column segment in
    `bilara_multi_column_html()` — check `sutta_to_segments_json()` in
    `app_data.rs` for where each column's segments are built so the function
    is applied once per segment, not twice.
- **CSS** in `assets/sass/_suttacentral.sass` (and the column cells): `.ssp-line
  { display: block }`, `.ssp-indent-N { padding-left: <N × step> }` with a
  hanging indent for wrapped lines. Compare with dhammatalks.org's look
  (Dhp 186–187, Thag 10.5) and the screenshots in the PRD's §6a. `make sass`.
- No change to stored data, `content_plain`, or the API output.

- [x] 7.0 Render line breaks and indentation in verse segments (Rust renderer + CSS), in the Solo, Lines and Columns layouts (PRD FR 34–40, 33a–33c)
  - [x] 7.1 Read `bilara_text_to_segments()`, `bilara_multi_column_html()` and `AppData::sutta_to_segments_json()` and list where a segment's final HTML is produced on each path; pick the single point per path where `render_verse_lines()` is applied. Both paths go through `bilara_text_to_segments()` (Solo wraps in the template, the multi-column path does not), so the one point is there, after `insert_markers()` and before the variant / gloss marks. It now always receives the record's template (to tell verse segments) and a new `wrap_in_template` flag.
  - [x] 7.2 Implement `render_verse_lines()` as specified.
  - [x] 7.3 Apply it on both paths at the points from 7.1.
  - [x] 7.4 Add the CSS and rebuild the stylesheets (`make sass`).
  - [x] 7.5 Unit tests (PRD FR 33c): Ṭhānissaro's `dhp187:3` text gives three spans; a test string with tabs gives the right `ssp-indent-N`; a trailing `\n` adds nothing; Sujato-style and Pāli segments are byte-identical to today; a footnote marker inserted at a word in line 2 stays in line 2's span; a prose segment (`<p>{}` template) is unchanged; Solo and multi-column output contain the same line spans.
  - [x] 7.6 Test that `content_plain` produced by `bilara_html_and_plain_text()` for a segment with `\n` and `\t` holds no tab and no newline-joined words.
  - [x] 7.7 Check that the find bar (`src-ts/find.ts`) still matches a phrase that crosses two lines of one segment, and that tap-to-lookup (`src-ts/pali_word.ts`) still selects a word at a line start; add a jest test where the existing suites make that practical, otherwise list it for the manual check in 9.6. Found and fixed: the spans must keep the `\n` between them, or the page text reads `He isone who`. Jest tests in `find.test.ts` and `pali_word.test.ts`; the on-device tap check stays in 9.6.
  - [x] 7.8 Update `docs/simsapa-cli-and-localhost-api.md` §13.4 / §13.5 (`\n` = line break, leading `\t` = indent level; self-contained wording), and run `cd backend_cli && cargo test test_bundled_doc_has_no_repo_references`.
  - [x] 7.9 `make build -B` and `cd backend && cargo test` pass.

### 8.0 specs

Depends on: 7.0 (the contract). Work happens in the **suttas-core repository**
(`bootstrap-assets-resources/suttas-core`, remote
`git.unxr.net/adicco/suttas-core`) under its own `AGENTS.md`: bun /
TypeScript, deterministic builds, no model calls, no network without
approval, systematic rules in the parser. PRD FR 41–43.

- Parser (`src/sources/than/page.ts`): a verse `<p class="vN">` line keeps its
  level as leading tabs in the raw text; unclassed `<p>` = level 0. The
  class → level mapping (v1…v8) is decided by comparing the vendored pages
  with dhammatalks.org's rendering (Thag 10.5: `v2` = 1 step, `v4` = 2 steps;
  `v1`, `v3`, `v5`–`v8` to be established) and written into
  `docs/parsers/than.md`, replacing the "parser trims it" note.
- `textHash` collapses whitespace → no segmentation goes stale. Confirm with
  `src/check/stale.ts` before and after.
- Build: give each segment line that starts a raw verse line that line's
  tabs (letters/digits match, as `src/segment/footnotes.ts` anchors notes);
  continuation lines get none; unmatched lines are reported in the build
  summary.
- Output: `sutta_segments.content` carries the tabs. A new dated DB folder in
  Simsapa's `bootstrap-assets-resources/suttas-core-db/`, `current` repointed.

- [x] 8.0 Carry Ṭhānissaro's verse indentation through suttas-core into its segment text, regenerate the suttas-core database, and confirm Simsapa imports it (PRD FR 41–43)
  - [x] 8.1 Read `suttas-core/AGENTS.md`, `docs/parsers/than.md`, `docs/data-layers.md` and `src/sources/than/page.ts`; confirm with the maintainer how changes to that repo are committed (it has its own remote).
  - [x] 8.2 Establish the class → level mapping for `v1`–`v8` from the vendored pages (and, with approval, dhammatalks.org's stylesheet); record it in `docs/parsers/than.md`. Mapping from dhammatalks.org's stylesheet: `p.vN` padding `(2 + 4N)%`, so `vN` = level N (v2 is level 2, not 1); `<pre>` verse: one level per leading tab. The stylesheet copy is kept in the ignored `out/than-css/` (suttas-core keeps only parser inputs in `data/sources/`). Behind `--verse-indent`, off by default (maintainer's decision), so upstream output is byte-identical without it.
  - [x] 8.3 Change the parser to emit leading tabs per verse line; add a parser test on Dhp 186–187 and Thag 10.5.
  - [x] 8.4 Run `src/check/stale.ts` and `compare-raws.ts --translator than`: no segmentation newly stale; the 139 `whitespace_only` rows are now explained by the indentation. `stale.ts`: 0 stale; no raw's `textHash` changes with the option. Not done: re-classifying the 139 `whitespace_only` rows needs the legacy DB (`htmx-astro-experiment/suttas.sqlite`), which is only on the suttas-core author's machine; not pursued, since only Simsapa uses `--verse-indent`.
  - [x] 8.5 Implement the build step that applies the tabs to segment lines, with a test that a continuation line gets none and an unmatched line is reported. Also restores a raw verse line break that legacy segmentation joined with a space, in verse segments only (maintainer's decision: keep his line layout), and only for sources that honour the option (`Source.verseIndent`: than); 3,801 restored, 6,830 lines indented, 23 indented lines still joined, 8 lines in 7 segments not found (reported).
  - [x] 8.6 `bunx tsc --noEmit -p .` and the suttas-core tests pass; run the build and read its report of unmatched lines. 75 tests pass.
  - [x] 8.7 Copy the new database into `suttas-core-db/<date>/` and repoint `current`.
  - [x] 8.8 In Simsapa, add a test (skips when the DB is absent) that `SuttasCoreImporter::build_record()` keeps leading tabs: `dhp179-196` `than` has `dhp187:3` starting a line with `\t` before *one who delights*.

### 9.0 specs

Depends on: 1.0–8.0 (8.0 only for the indentation checks).

- Docs describe current behaviour only, no history narrative; measurements go
  in the commit message.
- The bootstrap run is long and rewrites `dist/`. Ask the maintainer before
  starting a full run; prefer re-running only the affected steps if a
  subcommand allows it. Never commit the resulting databases.

- [ ] 9.0 Documentation and the end-to-end check: new doc, updates to `suttas-core-import.md`, the text-processing doc, `CLAUDE.md`, `PROJECT_MAP.md`, the API doc, and a bootstrap run of the affected steps with the success metrics checked (PRD FR 30–33a, §8)
  - [ ] 9.1 Write `docs/munindo-nyanadipa-segmentation.md` (including the verse-segment line contract and `render_verse_lines()`, FR 33a), and add the pointer section to `docs/sutta-display-settings-and-multi-column-view.md`: file format and `source` values, hash definitions, the placement rules, title placement and why (the multi-column view shows no record title), the footnote file, the verse-group rule and the three importers that apply it, the check command and when to run it, and the traps (key order, trailing spaces, never derive alignment from Munindo's numbers).
  - [ ] 9.2 Update `docs/suttas-core-import.md`: §1 position text (the Munindo / Nyanadipa step now reads segment files), §3 Ṭhānissaro Dhammapada ranges and chapter titles, §5 markup list (Nyanadipa now uses `ssp-fn`, not `sup.footnote-reference`).
  - [ ] 9.3 Update `docs/text-processing-for-contains-match-and-fulltext-match-search.md` where it describes these two sources as HTML.
  - [ ] 9.3a Update the comments that name Nyanadipa as a user of `sup.footnote-reference`: `assets/sass/_footnote_bottom_bar.scss` (~249), `src-ts/footnote_modal.ts` (~146), `src-ts/footnote_bottom_bar.test.ts` (~4). Library books still use that markup, so only the comments change.
  - [ ] 9.4 Add the new doc to the notable-docs list in `CLAUDE.md`, and the new module and command to `PROJECT_MAP.md`.
  - [ ] 9.5 With the maintainer's go-ahead, run the bootstrap (or the affected steps) and check the success metrics with SQL against the new `appdata.sqlite3`:
    - [ ] 9.5.1 26 Munindo and 36 Nyanadipa records with `content_json` and NULL `content_html`.
    - [ ] 9.5.2 Every Dhammapada heading segment in the `pli/ms`, `en/sujato`, `en/thanissaro` and `en/munindo` records carries its range, and the ranges cover 1–423. Select the headings by the rule of `dhp_verse_group_ranges()` (`dhpN:0`, and the first verse's `0.4`), not by `LIKE '%:0%'`: `dhp416:5.0` (*Jotikattheravatthu*, the Pāli root's repeat of Dhp 416) is a verse-story heading that by design gets no range. The new doc (9.1) states this exception.
    - [ ] 9.5.3 26 Munindo, 26 Ṭhānissaro Dhammapada and 36 Nyanadipa records have a non-empty page-title segment.
    - [ ] 9.5.4 The navigation tree build reports no new missing leaves for these uids.
    - [ ] 9.5.5 `dhp1-20/pli/ms` and `snp4.2/pli/ms` default to `…/en/sujato`.
    - [ ] 9.5.6 `git status` shows no `*.sqlite3` staged.
  - [ ] 9.6 Ask the maintainer to open three Munindo chapters and three Nyanadipa suttas in the Lines and Columns layouts, and check the alignment, the verse numbers, the titles, the line breaks and the footnote markers; and Ṭhānissaro's Dhp 186–187 and Thag 10.5 and Bodhi's SN 1.75 for line breaks and indentation against the published pages (PRD §6, §6a, §8). Include the find-bar and tap-to-lookup checks deferred from 7.7, on desktop and on a mobile device.
