# DPD releases: the bootstrap inputs, the `dpd_res` table and its route

Simsapa ships the Digital Pāḷi Dictionary (DPD) in three parts that must come
from the **same DPD release**:

- **Structured data**: `dpd.sqlite3` (headwords, roots, lookup, bold
  definitions), migrated from upstream's `dpd.db`.
- **Rendered pages**: the `dict_words` rows with `dict_label = "dpd"` in
  `dictionaries.sqlite3`, imported from the StarDict export (`dpd/`) inside
  `dpd-goldendict.zip`.
- **Page files**: the StarDict export's `res/` folder (stylesheet, Inter fonts,
  page scripts and the `family_*_json.js` word-family data) that the pages
  load. These are stored in the `dpd_res` table of `dpd.sqlite3`, so they
  always arrive with the data of their own release.

## 1. Preparing a DPD release for the bootstrap

The bootstrap reads the **already extracted** files under
`bootstrap-assets-resources/dpd-db-for-bootstrap/current/`, and never extracts
an archive itself (that would add the decompression time to every run). On a
new DPD release, once:

1. Download `dpd.db.tar.xz` and `dpd-goldendict.zip` from the release at
   `https://github.com/digitalpalidictionary/dpd-db/releases`.
2. Make a folder named by the release date, e.g. `dpd-db-for-bootstrap/2026-10-26/`,
   and extract into it:
   - `dpd.db.tar.xz` → `dpd.db`;
   - `dpd-goldendict.zip` → at least its `dpd/` folder (`dpd.ifo`, `dpd.idx`,
     `dpd.dict.dz`, `dpd.syn.dz` and `res/`). The zip's other folders
     (`dpd-grammar/`, `dpd-deconstructor/`, …) are not imported.
3. Point the `current` symlink at it: `ln -sfn 2026-10-26 current`.

Nothing in the repo is edited by hand: the `res/` conversions below are part
of the import. The paths are the constants `DPD_DB`, `DPD_STARDICT_DIR` and
`DPD_RES_DIR` in `bootstrap_cli/src/bootstrap/source_paths.rs`, and the
preflight (`bootstrap_sources()`) lists all three as **required** under the
`dpd` step, so a missing one is reported before `dist/` is emptied.

## 2. The import

`dpd_bootstrap()` imports the StarDict pages into `dictionaries.sqlite3` first,
then `import_migrate_dpd()` (`backend/src/db/dpd.rs`) migrates `dpd.db` into
`dpd.sqlite3`. It takes every input explicitly (input and output paths, the
dictionaries database, the DPD `dictionary_id`, the `res/` folder) and uses no
global app data. In order:

1. `migrate_dpd()`: schema changes, niggahīta (ṃ → ṁ) in every text column,
   the `dictionary_id`.
2. Move `dpd.db` to the output `dpd.sqlite3`.
3. Import `res/` into `dpd_res` (§3), then the `<link href>` check (§3).
4. The bold-definition derived columns, then `construction_summary`, cut from
   the pages in the given dictionaries database
   ([dpd-construction-summary.md](./dpd-construction-summary.md)).
5. The B-tree and FTS5 indexes (`create_dpd_indexes()`).
6. Write `db_info.dpd_release_version` (e.g. `v0.4.20261026`) into the
   `version` column of the DPD row of `dictionaries.dictionaries`, so the
   pages and the data both record their release.

## 3. The `dpd_res` table

```sql
CREATE TABLE dpd_res (
    path TEXT PRIMARY KEY NOT NULL,  -- relative to res/, forward slashes
    mime_type TEXT NOT NULL,
    content BLOB NOT NULL
);
```

Created by the migration (`backend/src/db/dpd_res.rs`, `import_dpd_res_dir()`),
not by a Diesel migration: `dpd.sqlite3` has none. The table is dropped and
recreated on each import.

- **Every regular file** of `res/` is imported, subfolders included; there is no
  list of names. A missing or empty folder is an error naming it.
- **Text files** (`.css`, `.js`, `.json`) go through `consistent_niggahita()`
  (ṃ → ṁ, ŋ → ṁ), like the rest of the DPD data, so the Idioms and Family
  sections built from `family_*_json.js` show ṁ. A file that is not UTF-8 is an
  error. **Other files** (fonts, icons) are stored byte for byte.
- **The stylesheet is otherwise stored as DPD ships it.** Its `@font-face`
  `url('Inter-…ttf')` is relative, so it resolves against the stylesheet's own
  URL (`{api_url}/dpd_res/dpd-css-and-fonts.css`) on whatever port the server
  took. Never store a port in the table: the server starts at 4848 and takes
  the next free port when 4848 is busy. Simsapa's own rules for DPD pages
  (`a.word_link`, for the links `convert_dpd_epd_word_links()` makes) are in
  `assets/css/dictionary.css`, injected into every dictionary page after DPD's
  stylesheet.
- **The link check** (`check_dpd_link_hrefs()`): every `<link href="…">` in a
  sample of about 2,000 DPD pages, spread over the id range, must name a stored
  path. A missing file fails the import and names the file. It is reported,
  never repaired.

## 4. Serving: `GET /dpd_res/<path..>`

The page renderer (`render_word_html_by_uid()` in `backend/src/app_data.rs`)
rewrites every `<link href>` of a DPD page to `{api_url}/dpd_res/<href>`. The
page's inline loader turns each `<link class="load_js">` into a `<script>` with
the same `src`, so the scripts follow the rewrite. User-imported dictionaries
are not routed there: their files are in `dict_resources`.

The route is in `simsapa_api` (`api/src/routes/dpd_res.rs`), so `simsapa serve`
renders complete DPD pages too. It answers with the stored `mime_type`, `404`
when the path is not stored or the installed `dpd.sqlite3` has no `dpd_res`
table, and `500` on a database error. It never panics.

**The files are served from memory.** A headword or root page (about half of
the DPD pages) links 13 or 14 of the files, about 13 MB with the word-family
data; the others link only the stylesheet. Reading that from SQLite for every
page opened would be wasted work, so the route reads `DPD_RES_CACHE`, a
process-wide copy of the table:

- loaded by `DpdDbHandle::warm_dpd_res_cache()` on a background thread after
  startup (the GUI in `SuttaBridge::load_db()`'s thread, after `db_loaded`;
  `simsapa serve` before launching), or by the first request if that has not
  finished. A load lock makes the table read once either way;
- a failed load is not kept, so the next request tries again; a `dpd.sqlite3`
  without the table is kept as an empty set;
- **never reloaded.** `dpd.sqlite3` is only replaced while the app is not
  running, so the copy cannot go stale within a process. Code that replaces
  `dpd.sqlite3` must therefore never run in a process that has loaded the copy
  (no `/dpd_res/` request and no `load_db()` before the swap).

There is no fallback to files compiled into the binary: a `dpd.sqlite3` without
the table gives pages without their stylesheet and scripts, and the route
answers `404`.

## 5. Stored gloss selections across DPD releases

A DPD headword keeps its id across releases while its `lemma_1` label can
change, so stored gloss selections use the numeric `<id>/dpd` uid, with the
lemma beside it in files. See the "uid two-lane gotcha" in
[gloss-ai-word-selection.md](./gloss-ai-word-selection.md).
