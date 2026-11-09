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

## 6. The upgrade pipeline (`backend/src/dpd_upgrade/`)

One implementation, `run_dpd_upgrade()`, for the three entry points
(`bootstrap_cli rebuild-dpd`, `simsapa dpd upgrade`, the GUI's startup upgrade
window). It takes its files as a `DpdUpgradePaths` and opens only its own
connections, so it runs before `init_app_data()` and needs no global app data.

**Releases** (`releases.rs`). `GET /repos/digitalpalidictionary/dpd-db/releases?per_page=100`
(the API's default page is 30, which a `--version` naming an older release
could miss), not cached. A candidate's tag parses as a version, has the installed DPD's
major and minor (`db_info.dpd_release_version`), is newer, is not a draft or a
prerelease, and has both `dpd.db.tar.xz` and `dpd-goldendict.zip` (a
`.tar.bz2`-only release is not a candidate). The newest candidate is offered.
An installed version that is unknown or does not parse gives **no** candidate.

**Steps**, each named in an error (`UpgradeError.step`):

| Step | Writes | What it does |
|---|---|---|
| `check` | staging folder | installed version, compatibility (of a release's tag, or of a local extracted `dpd.db`, read with `mode=ro` so the user's file is never written), free space, the lock |
| `download` | staging | both assets, 5 connection attempts with backoff |
| `extract` | staging | `dpd.db` out of the `.tar.xz` (pure-Rust `lzma-rust2`), then the rest of the xz stream read and discarded: the decoder checks a block's CRC64 only when it reaches the block's end, so stopping after the tar entry would leave the last block (the tail of `dpd.db`) and the stream footer unchecked. Only the `dpd/` member of the zip; a local `dpd.db` is **copied**, a local `dpd/` folder is read in place |
| `verify` | — | `dpd.db`'s version is the tag (local: compatible and newer), `dpd.ifo` and a non-empty `res/` exist |
| `prepare_html` | staging | a staging `dictionaries` database (the Diesel migrations, no FTS5 tables), the installed DPD row copied with its id, the StarDict pages, then the bootstrap's three conversions in the bootstrap's order. The conversions read `sutta_info` from the **unmigrated** `dpd.db`, so this runs before `prepare_data` |
| `prepare_data` | staging | `import_migrate_dpd()` into a staged `dpd.sqlite3`; summaries cut from the **staged** pages; FTS5 integrity checks |
| `apply` | installed | gloss uids, pages, `dpd.sqlite3` (below) |
| `reindex` | installed | `ANALYZE` both databases; the `pli` dictionary index: `append_dict_label_to_dict_index(…, "pli", "dpd")`, then the bold definitions deleted by `is_bold_definition = true` and appended again. Never `build_dict_index`, which clears user-imported Pāli dictionaries. A failure here has the code `reindex_failed`: the new release is installed, so the upgrade cannot be run again ("not newer"), and the message (`REINDEX_RECOVERY`) names the full rebuild, Settings > Rebuild Search Index or `simsapa index rebuild`; not `index rebuild --area dict-words --lang pli`, which leaves out the bold definitions |

Nothing before `apply` writes an installed file. The cancel flag is checked
between steps and between batches inside the long ones (`BatchProgress`); from
`apply` on the run is not cancelled.

**Apply** (`apply.rs`), in this order:

1. Lemma-form DPD `selected_uid`s in `gloss_word_context_cache` and
   `gloss_phrase_selections` become numeric, resolved against the **installed**
   `dpd.sqlite3` (the release they were written against). Unresolved ones are
   reported and left. Numeric uids are right for either release, so this goes
   first.
2. One transaction on `dictionaries.sqlite3`: delete the DPD rows, insert the
   staged ones (new rowids, same `dictionary_id`), set the DPD row's
   `version`. The six FTS5 sync triggers are dropped first, both FTS5 tables
   are rebuilt from `dict_words` (`'rebuild'`) and the triggers are recreated
   from the embedded `dictionaries-fts5-indexes.sql`
   (`FtsScript::trigger_statements()`, never a second copy of their SQL), all
   in the transaction. DPD is 96% of the tables' text, so tokenizing it once
   beats the triggers' `'delete'` of every old row and insert of every new one,
   and the file does not grow. Then `integrity-check` on both. The column
   lists are the columns both databases have, by name: in the GUI the
   installed database may be a migration behind this build.
3. Leftover `-journal`/`-wal`/`-shm` files of the old `dpd.sqlite3` are
   removed (a hot journal would be replayed into the new file), and the staged
   file is renamed over it. Every handle to it must be closed first (Windows).

**Result order.** The new DPD rows get row ids after every other
dictionary's, while DPD keeps its `dictionary_id`. Dictionary results are
therefore ordered by `dictionary_id` (the dictionary's relevance rank: the
bootstrap creates DPD first, DPPN second, and user imports get later ids), then
by row id, never by row id alone and never by `dict_label`
(`sort_by_dictionary_rank()` in `backend/src/query_task.rs`), so DPD still
comes first after an upgrade.

After the run, the stored selections whose headword id the new release removed
are reported (`removed_headword_selections`), not changed.

**Staging folder**: `app-assets/dpd-upgrade-staging/`, on the same volume as
`dpd.sqlite3` so the replacement is a rename. A file in it is opened with
`journal_mode = OFF` and `synchronous = OFF` by `fts_scripts::open_db_path()`,
which decides by location (`is_staged_db_path()`): the staged files are thrown
away on any failure, and both pragmas are per connection. An installed database
is never in that folder. The folder holds `upgrade.lock` (`pid <n>`), rewritten
every 30 s by a heartbeat thread. The lock is held while its process is alive
and the lock is under 5 minutes old (`lock_is_held()`). A dead pid releases it
at once, however new the lock: an upgrade killed a minute ago (the app swiped
away) must not block the next start, which would otherwise end with "already
running". Where the platform cannot tell whether a pid exists (Windows), the
age alone decides. The lock with this process's own pid is held only while
this process runs an upgrade. A run removes a leftover folder before it
starts, and refuses to start while the lock is held. The folder is removed when the run ends, on
success, failure and cancel; `init_app_data()` also sweeps a stale one.

**Free space** (`estimate_space()`): the larger of the unpacking peak (both
archives, `dpd.db`, the `dpd/` folder) and the apply peak (the staged
`dpd.sqlite3`, the `dpd/` folder, the staging pages database, and the page
replacement's rollback journal, 1.2 × the pages), plus 15%. The extracted
`dpd.db` and the pages are estimated from the installed `dpd.sqlite3` and the
installed DPD pages.

**Consistency** (`dpd_versions_mismatch()`): the DPD row's `version` and
`dpd_release_version` differ when a run was killed between steps 2 and 3 of
the apply. Reported, never repaired. An empty `version` is not a mismatch.

## 7. Entry points

### 7.1 `bootstrap_cli rebuild-dpd`

Replaces the DPD of an existing `SIMSAPA_DIR` (normally the dist,
`dist/simsapa`) with the extracted release in
`dpd-db-for-bootstrap/current/`, without a full bootstrap:

``` sh
cd bootstrap_cli
cargo run --release -- rebuild-dpd            # SIMSAPA_DIR from .env or the environment
cargo run --release -- --simsapa-dir <dir> rebuild-dpd [--limit N]
```

- It first checks the three release inputs (`dpd_release_sources()` in
  `preflight.rs`, the same entries the bootstrap's `dpd` step lists, without
  the DPPN) and fails naming every missing one before anything is written.
- Then it runs `run_dpd_upgrade()` with the two **extracted** inputs as a
  local source, so nothing is downloaded or unpacked, and with
  `allow_same_version`: the release may be the one already installed. An
  older or incompatible release is still refused.
- It writes no release archives, and opens no `AppData` (the pipeline uses its
  own connections), so `init_app_data()` is skipped for it in `main.rs`.
- Run it with the app closed. The pipeline's `upgrade.lock` stops a second
  run, but nothing stops the app from holding the databases open.
- `--limit N` imports only N StarDict entries and bold definitions, for a
  quick test of the wiring; the result is not a usable dictionary.
- Use `cargo run --release`: in a debug build the StarDict parse and the
  footer strip take several minutes each instead of under a minute.
- Disk: the run needs the free space `estimate_space()` prints (about 6 GB for
  v0.4.20261026) on the volume of `SIMSAPA_DIR`.

The usual round after a new DPD release is §1 (extract, point `current` at
it), then `rebuild-dpd` on the dist to test the release in the app, and a full
bootstrap for the database release.
