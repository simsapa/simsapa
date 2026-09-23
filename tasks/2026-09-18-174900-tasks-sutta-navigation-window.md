# Tasks — Sutta Navigation Window

**PRD:** [2026-09-18-174900-prd---sutta-navigation-window.md](./2026-09-18-174900-prd---sutta-navigation-window.md)
**Feature name:** `sutta-navigation-window`

---

## Relevant Files

### New files

- `backend/migrations/appdata/2026-09-19-000000_navigation_tree/up.sql` — the node table, its indexes and the path column (FR-9, FR-11).
- `backend/migrations/appdata/2026-09-19-000000_navigation_tree/down.sql` — matching `DROP TABLE`s.
- `backend/src/navigation_tree.rs` — runtime read layer: level query, filter query, node lookup, translation counts, table-absent tolerance (FR-10, FR-11, FR-13, FR-48).
- `backend/src/navigation_tree/tests.rs` — unit tests for the read layer against a temp DB built from the real `up.sql`, so schema/`table!` drift fails here rather than at runtime.
- `cli/src/bootstrap/navigation_tree/mod.rs` — shared node writer, `node_uid` derivation, insert batching, `ANALYZE` (FR-1, FR-7, FR-12, T-3).
- `cli/src/bootstrap/navigation_tree/sc_spine.rs` — the SuttaCentral spine builder (FR-2, FR-6).
- `cli/src/bootstrap/navigation_tree/cst_spine.rs` — the CST toc spine builder: toc walk, node-uid derivation, level labels, the 1:1 `romn/` cross-check (FR-3, FR-4, FR-5, FR-6).
- `cli/src/bootstrap/navigation_tree/coverage_report.rs` — the FR-21 coverage report, filled by the import and completed by the spine.
- `cli/src/bootstrap/cst_coverage.rs` — the four coverage classes computed from `fragments.sqlite3`, and the toc-driven division splitter (FR-14, FR-15, FR-20).
- `cpp/sutta_navigation_window.cpp` / `.h` — the window wrapper (FR-23, FR-24).
- `bridges/assets/qml/SuttaNavigationWindow.qml` — the window (FR-27 to FR-46).
- `bridges/assets/qml/NavigationNodeRow.qml` — leaf/branch row rendering (FR-43 to FR-46, FR-49).
- `bridges/assets/qml/NavigationBreadcrumbBar.qml` — breadcrumb bar with middle-segment elision (FR-35, FR-37a).
- `docs/sutta-navigation-tree.md` — the two spines, `VERIFIED_CST_FILES` and the coverage classes, the dual-identity rule, the split rule, node-uid stability, the rebuild commands (T-8). **Created in task 4**; task 9.8 extends it.

### Modified files

- `backend/src/db/appdata_schema.rs` — Diesel table declarations for the new tables.
- `backend/src/lib.rs` — register the `navigation_tree` module.
- `backend/src/app_data.rs` — hook translation-count cache invalidation into `refresh_language_caches()` (FR-48).
- `cli/src/bootstrap/mod.rs` — call the tree build after the sutta import (T-1).
- `cli/src/main.rs` — the `build-navigation-tree` command, so the tree can be
  rebuilt on an existing appdata DB without a full bootstrap (it runs pending
  migrations first). `--sc-data-dir` and `--cst-xml-dir` are independent and
  both optional, so the CST spine can be rebuilt without ArangoDB. This is also
  how success metric 6 is checked.
- `backend/src/app_settings.rs` — the persisted `show_navigation_blurbs` toggle (FR-31).
- `bridges/src/sutta_bridge.rs` — the `nav_tree_*` readers and the blurb-toggle accessors.
- `bridges/build.rs` — the three navigation QML files in `qml_files`.
- `cli/src/bootstrap/suttacentral.rs` — `get_titles()` made `pub` for FR-2b.
- `cli/src/bootstrap/tipitaka_xml.rs` — the coverage class applied *within* the verified-file list, per-toc-leaf division records, and the whole-file fallback (FR-14, FR-15, FR-19, FR-20, FR-22).
- `cli/src/bootstrap/cst_coverage.rs` — holds **`VERIFIED_CST_FILES`**, the renamed `CHECKED_XML_WHITELIST` (same 30 entries), plus the coverage classes and the division splitter.
- `cli/src/main.rs` — also the `import-cst` command, which re-runs the CST import on an existing appdata DB (it clears the existing `%/pli/cst` rows first). Re-running it requires `index rebuild --area suttas --lang pli` afterwards.
- `backend/tests/test_search_normalization.rs` — the Dhovana uid is now the division record, not the removed whole-file one.
- `backend/tests/data/fulltext_search_so_ce_evam_vadeyya.json` — BM25 scores regenerated for the re-indexed corpus (same uids, same order).
- `tipitaka-xml-parser/src/…` — only if a split helper belongs beside `nikaya_structure.rs` / `encoding.rs`; do not add a second decoder (FR-4).
- `bridges/src/sutta_bridge.rs` — the bridge functions (FR-50, FR-52).
- `bridges/src/api.rs` — declares `callback_open_sutta_navigation_window()` in the `gui.h` extern block (FR-26).
- `cpp/gui.h` — the matching C++ declaration.
- `bridges/build.rs` — register the new QML files in `qml_files`.
- `bridges/assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` — `qmllint` stubs (T-7).
- `cpp/window_manager.cpp` / `.h` — `create_sutta_navigation_window()`, the new list, close handling (FR-23 to FR-25).
- `CMakeLists.txt` — `cpp/sutta_navigation_window.cpp` added to `cpp_files`.
- `cpp/gui.cpp` — confirm the session save cannot see the new window (FR-25, resolved question 6).
- `bridges/assets/qml/SuttaSearchWindow.qml` — menu-bar and drawer entry points (FR-26).
- `docs/window-lifecycle-and-reuse.md`, `docs/user-data-and-sqlite-analyze.md`, `PROJECT_MAP.md` — amendments (T-8).

### Notes

- **Measured while assessing the codebase, before writing any code:**
  - `romn/toc1.xml` … `toc4.xml` are **UTF-8 with a BOM** even though their XML
    declaration says `encoding="UTF-16"`; `romn/cscd/*.toc.xml` really are
    UTF-16. `tipitaka_xml_parser::encoding::read_xml_file` detects by BOM and
    handles both — which is exactly why FR-4 forbids a second decoder. A naïve
    `iconv -f UTF-16` on `toc2.xml` produces mojibake.
  - `romn/cscd/` contains **only** `*.toc.xml` files — the split documents
    (`s0305m.mul0.xml` …) exist under `deva/cscd/` only, as the PRD states.
  - **Two different `romn/` directories.** The 217 *source* XML files are in
    `tipitaka-xml/romn/` (where `TipitakaXmlImporter` already points);
    `tipitaka-xml/tipitaka.org/romn/` holds only 5 XML files (`toc1`–`toc4` and
    `tipitaka_toc.xml`), and the 217 `*.toc.xml` are in
    `tipitaka.org/romn/cscd/`. The pairing is exactly 1:1 — measured, zero
    unmatched either way.
  - **All 217 source files are already imported**: 187 as whole-file
    `…xml/pli/cst` records and 30 (the whitelist) as 893 `.att` + 899 `.tik`
    fragments, 3,361 CST rows in the shipped DB. Task 4 is a *subdivision* of the
    187, not a bulk import. (An earlier draft of this note said only 30 of 217
    were imported; that was wrong.)
  - **`CHECKED_XML_WHITELIST` is a hard-coded Rust array — and it is a
    *verification* gate, not a coverage predicate.** It records which files a
    human checked after running `tipitaka-xml-parser`'s validations; the parser
    is a work in progress that produces wrong splits and wrong uid assignments
    on structures it does not handle. **It is kept and renamed
    `VERIFIED_CST_FILES`** (`cli/src/bootstrap/cst_coverage.rs`, same 30
    entries) — "checked" collided with `frag_review = 'checked'`, which means
    something else entirely. An earlier revision of task 4 deleted it per FR-15
    and admitted **3,055 fragments from 20 unverified files**; FR-15 has been
    corrected in the PRD. Only a listed file contributes fragment records.
    Measured per file over `fragments.sqlite3`, *within* that list: **26** fully
    mapped, **4** partially mapped (`s0101a.att.xml` / `s0101t.tik.xml` 13 of
    14, `s0201a.att.xml` / `s0201t.tik.xml` 50 of 51). Because Phase 2 excluded
    every listed file, those four suttas were silently absent from the app; they
    are now covered by division records. The predicate inside the list is
    **full coverage**, not "has any `sc_code`": the latter would import 2 of
    `s0401m.mul.xml`'s 33 suttas and lose 31.
  - **Whole-file records are 74 MB of the `suttas` table's 163 MB of
    `content_plain`** — 45% of all indexable sutta text, carried by a *trigram*
    FTS5 index and an 841 MB Tantivy index. That is why a coarser record is kept
    only where no finer partition covers the file (FR-14): keeping both would
    grow both indexes by ~45% and return every CST hit twice.
  - **Gaplessness was checked against the file text, not just the fragment
    rows.** For all 28 fully-mapped files the Sutta fragments are contiguous
    with **zero internal gap lines**, leaving a 5–8 line head (XML declaration,
    nikāya/book title markup) and a 3-line tail (closing tags). Dropping their
    whole-file record loses only boilerplate. The bootstrap must re-derive this
    per file — an all-mapped file with an interior hole is *not* fully covered.
  - **The FR-20 partition method is settled by measurement**, not left open:
    romn `rend="chapter"` alone matches the toc leaf count for 169 of 217 files
    (47 at leaves = chapters + 1, one below), but **with the leading-division
    rule applied it matches for 216 of 217** — every one of those 47 files opens
    with a titled division the chapter markers do not open (`abh01a.att.xml`
    begins with `Ganthārambhakathā`, which is precisely its first toc leaf). The
    `deva/cscd/` count matches only 193. Use the romn markup. The sole residue
    is `abh05t.nrf.xml` (5 leaves, 6 chapters, plus a leading head), which falls
    to the FR-22 whole-file fallback — so success metric 3's zero is attainable.
    **Define "body text" explicitly**, since the count turns on it: a head of
    only `rend="nikaya"` / `"book"` / `"title"` / `"centre"` paragraphs is not a
    division.
  - **`MobileOverlayTracker` will not see this window, and should not.** It is
    instantiated inside `SuttaSearchWindow` (`SuttaSearchWindow.qml:259`) and
    finds popups in that window's overlay plus *declared* in-tree child windows.
    A separate-engine top-level window is neither — nor is any existing
    secondary window. This is a device check, not a code task.
  - **Translation counts need no cache.** Measured on the real 842 MB DB
    (21,761 sutta rows): the whole-table `substr(uid,…)` GROUP BY is **14 ms**,
    and 100 unioned indexed prefix-range queries are **9 ms** — both far inside
    FR-50's 100 ms. A cache would only add the stale-count defect FR-48 warns
    about.
  - `WindowManager` already has the `reuse_or_evict<T>()` template
    (`cpp/window_manager.cpp:643`) with the null-`m_root` eviction FR-24 needs.
  - `save_session_now()` iterates `sutta_search_windows` only, so the navigation
    window is structurally excluded — but read it and confirm, per resolved
    question 6, rather than assuming.
  - `app_data.rs:243 refresh_language_caches()` is the invalidation hook FR-48
    names.
  - **The SC spine is restricted to the Theravāda Pāli canon (FR-2a).**
    `structure/tree/sutta/` holds 67 collection trees, most of them Chinese,
    Sanskrit or Tibetan (`lzh-*`, `sht`, `xct-uv`, `t210`–`t213`, `sa-2`,
    `divy`) with no texts in Simsapa; the scope reaches 24. All 20 KN members
    *are* present in Pāli, so within scope there are no empty branches to prune.
  - **ArangoDB is already a bootstrap dependency, not a new one.** `arangors` is
    in `cli/Cargo.toml`, and `connect_to_arangodb()` / `get_titles()` live at
    `cli/src/bootstrap/suttacentral.rs:94` / `:202`. Non-goal 7 is about the
    shipped *app*, which never queries it.
- Rust tests: `cd backend && cargo test`. QML: `make qml-test` (lint does not
  fail the target; what matters is a *new* warning naming a file you touched).
- Do not run the GUI to test; build with `make build -B` and hand device/GUI
  checks to the user.
- Test DB for experiments:
  `bootstrap-assets-resources/dist/simsapa/app-assets/appdata.sqlite3`.
- **FR-51 has no task by design.** The tree build is a long operation in the
  CLI, not in the app, so the keep-screen-on rule does not apply. It becomes a
  requirement only if a future in-app rebuild is added, and then under a
  distinct holder name — non-goal 1 puts that out of scope here.

---

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, check it off in this markdown file by
changing `- [ ]` to `- [x]`. Update the file after completing each sub-task, not
just after completing an entire parent task.

---

## Tasks

### 1.0 Storage: migration, backend read layer, and honest empty-state

> **Specs to keep in mind.**
> **Schema:** one node table; every capability in the FR-9 column list must be
> expressible. Index `parent_uid`, index `node_uid` (unique), and carry an
> indexed materialized `path` column (ancestor `node_uid`s, delimiter-joined) so
> FR-11's whole-tree filter and breadcrumb reconstruction are one query.
> **Read API shape:** `children_of(parent_uid) -> Vec<NavigationNode>`,
> `filter_nodes(query, limit) -> Vec<NavigationMatch>` (node + breadcrumb
> titles), `node_by_uid(uid)`, `tree_is_available() -> bool`.
> **State:** none cached yet; counts come in task 8.
> **Depends on:** nothing. This task ships first precisely so tasks 2–4 have a
> writer target and task 5 has something to expose.

- [x] 1.0 Storage: migration, backend read layer, and honest empty-state
  - [x] 1.1 Read `docs/database-migrations.md` and the existing
        `backend/migrations/appdata/2026-08-13-000000_topic_index_data/` as the
        shape to copy; confirm there is no second list to register a migration in.
  - [x] 1.2 Create `backend/migrations/appdata/2026-09-19-000000_navigation_tree/`
        with `up.sql` creating the node table (FR-9 columns: `node_uid`,
        `parent_uid`, `sort_index`, `node_type`, `tree_source`, `title_pali`,
        `title_en`, `blurb`, `sutta_uid`, `cst_file`, `has_children`) plus the
        `path` column, and `down.sql` dropping it.
  - [x] 1.3 Add indexes in the same `up.sql`: unique on `node_uid`, plain on
        `parent_uid`, and on `path` (FR-10, FR-11). Note in a SQL comment why
        `has_children` is denormalized.
  - [x] 1.4 Add the Diesel `table!` declaration to
        `backend/src/db/appdata_schema.rs` and to
        `allow_tables_to_appear_in_same_query!` if it will be joined to `suttas`.
  - [x] 1.5 Create `backend/src/navigation_tree.rs` with the `NavigationNode`
        struct, `Queryable`/`Insertable` derives, and register the module in
        `backend/src/lib.rs`.
  - [x] 1.6 Implement `children_of(parent_uid: Option<&str>)` ordered by
        `sort_index` — one level, never a whole-tree load (FR-10).
  - [x] 1.7 Implement `filter_nodes(query, limit)` matching `title_pali`,
        `title_en` and `node_uid`/`sutta_uid` (FR-27), returning each match with
        its breadcrumb titles resolved from the `path` column (FR-11). Two
        queries is the right shape — matches with their path strings, then one
        `WHERE node_uid IN (…)` over the union of ancestors. Resolving ancestor
        *titles* from a delimiter-joined path inside a single statement needs a
        recursive CTE to split the string; do not build one for this.
  - [x] 1.8 Implement `node_by_uid()` and `tree_is_available()`; make **every**
        read return an empty/`false` result rather than an error when the table
        is missing or empty (FR-13) — a migration failure is non-fatal by design.
  - [x] 1.9 Add unit tests over a temp SQLite DB: level query ordering, filter
        matching all three fields, breadcrumb reconstruction, and the
        table-absent path returning empty instead of panicking.
  - [x] 1.10 `cd backend && cargo test` and `make build -B`; confirm the app
        still starts for the user with the new empty tables.

### 2.0 Bootstrap spine A — the SuttaCentral-derived tree from `sc-data`

> **Specs to keep in mind.**
> **Scope (FR-2a): the Theravāda Pāli canon only** — `sutta` →
> `long/dn`, `middle/mn`, `linked/sn`, `numbered/an` and all 20 `minor/kn`
> members; `vinaya` → the `pli-tv-vi` group; `abhidhamma` → the `pli-tv-ab`
> group. Everything else in `super-tree.json` (`da`, `ma`, `sa*`, `ea*`,
> `dharmapadas`, `minor-lzh`, `lzh-mg-vi`, `san-*`, `lzh-*-ab`) is a collection
> Simsapa has no texts for and is **not built**. Restrict at the source; do not
> build then prune.
> **Titles (FR-2b):** ArangoDB's `names` collection via the existing
> `get_titles(db, lang)` (`cli/src/bootstrap/suttacentral.rs:202`), **not**
> `uid_expansion.json`, which has one name per uid and cannot supply a
> Pāli/English pair. `uid_expansion.json` supplies the acronym and collection
> display name only. `arangors` is already a `cli/Cargo.toml` dependency.
> **Inputs (all under `bootstrap-assets-resources/sc-data/`):**
> `structure/tree/super-tree.json` (measured shape: an array of
> single-key objects, `[{"sutta": [{"long": ["dn","da","da-ot"]}, …]}, …]`,
> nesting further for `minor` → `kn` → members);
> `structure/tree/{sutta,vinaya,abhidhamma}/*-tree.json` (measured shape:
> single-key object whose value is an array mixing plain uid strings for leaves
> and single-key objects for branches, e.g. `sn` → `sn-sagathavaggasamyutta` →
> `sn1` → `sn1-nalavagga` → `["sn1.1", …]`);
> `additional-info/blurbs.json` (keyed by language, then `grouping`/uid);
> `misc/uid_expansion.json` (human-readable names).
> **Writer:** the shared node writer from task 1; `tree_source = 'sc'`.
> **Depends on:** 1.0 (table + writer target).

- [x] 2.0 Bootstrap spine A — build the SuttaCentral-derived tree from the local `sc-data` checkout
  - [x] 2.1 Create `cli/src/bootstrap/navigation_tree/mod.rs` with the shared
        node-insert writer (batched inserts inside one transaction), a
        `NodeBuilder` that assigns `sort_index` from source order (FR-7), and a
        `defect_report` collector that logs offending identifiers without
        repairing anything (FR-8).
  - [x] 2.2 Implement `node_uid` derivation for the SC spine from the source uid
        (`sutta`, `sutta/linked`, `sn`, `sn56`, `sn56.11`), documented in a
        module comment as stable across bootstraps (T-3). Never derive it from a
        row id or enumeration order.
  - [x] 2.3 Parse `super-tree.json` into the three SC roots and their grouping
        levels (FR-6), tolerating the mixed string/object array shape and the
        extra `minor` → `kn` nesting. **Apply the FR-2a scope table as a filter
        while walking**, so the non-Theravāda collections are never built; the
        scope reaches 24 of the 67 trees in `structure/tree/sutta/`. Keep the
        table in one place so widening it later is a one-line change.
  - [x] 2.4 Parse each referenced `*-tree.json` recursively, depth-agnostically —
        driven entirely by nesting, never by a hard-coded level count (T-5).
        Record where a referenced tree file is absent as a defect (FR-8).
  - [x] 2.5 Fill `title_pali` / `title_en` from ArangoDB's `names` collection
        via the existing `get_titles(db, lang)`
        (`cli/src/bootstrap/suttacentral.rs:202`) — `is_root == true` for Pāli,
        `lang == @language` for English (FR-2b). Use `misc/uid_expansion.json`
        for the **acronym** and a collection's display name only; it has one
        name per uid and cannot supply both titles. Fall back to the imported
        `suttas` rows (`title_pali` / `title`) for a leaf `names` does not
        cover, and where neither has one leave the field null rather than
        inventing a title (FR-8).
  - [x] 2.6 Load `additional-info/blurbs.json` (`en` only — other languages are
        out of scope, non-goal 3) and attach blurbs to both grouping nodes and
        text nodes.
  - [x] 2.7 Resolve each leaf's `sutta_uid` against the imported `suttas` rows
        and report leaves that do not resolve (FR-8); mark them unavailable
        rather than dropping them silently, so FR-42 can dim them.
  - [x] 2.8 Set `has_children` and the `path` column as part of the write, in one
        pass after the subtree is built.
  - [x] 2.9 Verify against the real data that KN's members build sensibly (T-6);
        where a structure cannot be derived, produce a flat list under the root
        and **write the deviation into the implementation notes**, not into a
        silent fallback. All 20 members have Pāli rows in the shipped DB (`kp` 9
        … `mil` 248), so an empty KN branch means a build defect, not missing
        data.
  - [x] 2.10 Call the SC spine build from `cli/src/bootstrap/mod.rs` **after** the
        sutta import (T-1); run it and record the node count per root.

#### Implementation notes — 2.0 (SC spine), measured 2026-09-19

Run with `simsapa_cli build-navigation-tree --db-path <appdata.sqlite3>
--sc-data-dir <sc-data>`, added so the tree can be rebuilt on an existing
database without re-running the whole bootstrap (it applies pending migrations
first). The bootstrap calls the same `build_sc_spine()`.

**Result: 8,762 nodes, 1.2 s.** Per root: `sutta` 6,650 (5 groups), `vinaya`
1,001, `abhidhamma` 1,111. Title coverage 8,757 Pāli / 7,052 English; 4,020
blurbs. Re-running replaces the spine in place (`replace_spine` deletes by
`tree_source` inside the transaction), so the build is idempotent.

**543 leaves have no text in the corpus** and are written with
`is_available = 0` — kept and dimmable (FR-42), never dropped. They are **538
Pātimokkha rules** (`pli-tv-bu-pm-*`, `pli-tv-bi-pm-*`: every individual rule,
under branches that do exist) plus 5 ranged Bhikkhunī-vibhaṅga uids
(`pli-tv-bi-vb-ss7-9` and four like it). These 5 are also the only nodes with no
title at all from any source, so they render by their uid. No other defect kind
was recorded: **zero** duplicate node uids, zero missing tree files, zero
malformed entries across all 37 in-scope collections.

**Two findings worth keeping:**

- *The English title fallback must be taken from an English row.* Filling it
  from the row chosen for opening (a Pāli row) copied `suttas.title`, which
  holds the **Pāli** name, into the English slot — 1,705 rows printed their Pāli
  title twice. `load_sutta_rows()` therefore returns three maps, and the two
  title fallbacks are indexed by language.
- *`~` is SuttaCentral's own "untitled" marker, not a lookup failure.* 54 leaves
  (`an5.303`, `an11.22-29`, …) have `title_pali = "~"`; the `names` row really
  is `"~ "` and so is the imported `suttas` row, while the English title reads
  "Untitled Discourse on Greed (1st)". Left verbatim — reported, never repaired.

**KN and the flat collections (T-6).** All 20 KN members build, and none needed
a fallback: where a collection is flat it is **flat in `sc-data` itself**, not a
structure this code failed to derive. Flat (leaves directly under the
collection): `kp` 9, `dhp` 26, `bv` 29, `mnd` 16, `pe` 9 — and, in abhidhamma,
`kv` 219, `ya` 77, `patthana` 728. The rest nest 2–4 levels
(`ja` 626 descendants, `tha-ap` 619, `thag` 321, `mil` 282). `dhp`'s 26
"children" are vagga-sized *texts* (`dhp1-20` Yamakavagga), not branches, which
is SuttaCentral's grouping and is left as it is (T-2).

**Vinaya (T-4) behaves as the PRD expects:** `pli-tv-vi` carries all six
members, each on its own spine, with no cross-root join.

### 3.0 Bootstrap spine B — the CST-derived tree from the toc files

> **Specs to keep in mind.**
> **Inputs (all under
> `bootstrap-assets-resources/tipitaka-org-vri-cst/tipitaka-xml/tipitaka.org/romn/`):**
> `toc1.xml` (Mūla, cross-check only), `toc2.xml` (Aṭṭhakathā), `toc3.xml`
> (Ṭīkā), `toc4.xml` (Añña), and `cscd/<book>.toc.xml`. A `<tree src="…">` is a
> branch continued in the named file; a `<tree text="…" action="cscd/….xml">` is
> a leaf. `text=` carries the displayed Pāli title.
> **Encoding:** `tipitaka_xml_parser::encoding::read_xml_file` for every file —
> `toc1`–`toc4` are UTF-8-with-BOM despite declaring UTF-16, the `cscd` tocs are
> UTF-16 (FR-4).
> **Level labels:** `NikayaStructure::from_nikaya_name` / `GroupType` in
> `tipitaka-xml-parser/src/nikaya_structure.rs` (FR-5), labelling the **CST**
> spine only (T-2).
> **Writer:** the shared writer from 2.1; `tree_source = 'cst'`.
> **Depends on:** 1.0, and 2.1–2.2 for the writer and uid conventions.

- [x] 3.0 Bootstrap spine B — build the CST-derived tree from the tipitaka.org toc files
  - [x] 3.1 Create `cli/src/bootstrap/navigation_tree/cst_spine.rs`; read every
        toc file through `read_xml_file` and assert in a test that a
        BOM-mislabelled `toc2.xml` and a real UTF-16 `cscd` toc both decode.
  - [x] 3.2 Write the recursive `<tree>` walker: `src=` → descend into that file,
        `action=` → leaf. Report a `src=` pointing at a missing file, and a
        duplicate node uid, as defects (FR-8).
  - [x] 3.3 Derive `node_uid` for CST nodes from the source identifiers — the
        toc file name plus the leaf's index within it
        (`cst:s0305m.mul.toc:3`) — and document the stability requirement (T-3).
  - [x] 3.4 Build the three CST roots Aṭṭhakathā / Ṭīkā / Añña from
        `toc2`/`toc3`/`toc4` and attach them beside the SC roots at the top level
        (FR-6). Use `toc1.xml` as a cross-check only, not as a fifth root.
  - [x] 3.5 Label each level from `NikayaStructure` / `GroupType` where the book
        is one those cover; fall back to the toc's own `text=` where not (FR-5,
        T-5). Do **not** normalize CST grouping toward SC grouping (T-2).
  - [x] 3.6 Set `cst_file` on every node that came from a named XML file, and
        `sort_index` from document order (FR-7).
  - [x] 3.7 Verify the Añña root against the real `toc4.xml` (T-6); a flat list
        under the root is acceptable, but note the deviation in the
        implementation notes.
  - [x] 3.8 Check the Vinaya case by hand — `vin02m1.mul.xml`, `vin02a1.att.xml`,
        `vin11t.nrf.xml` land under three different roots with different internal
        structures, and that is the intended outcome (T-4). Build **no**
        cross-root join.
  - [x] 3.9 Call the CST spine build from the bootstrap after spine A; record the
        node count per root and the defect count.

#### Implementation notes — 3.0 (CST spine), measured 2026-09-19

Run with `simsapa_cli build-navigation-tree --db-path <appdata.sqlite3>
--cst-xml-dir <tipitaka-xml>`. Both spine flags are now optional and
independent, so the CST spine can be rebuilt without ArangoDB running (the SC
spine needs it for titles). The bootstrap calls `build_cst_spine()` right after
`build_sc_spine()`.

**Result: 2,128 nodes, ~0.1 s.** Per root: Aṭṭhakathā 727, Ṭīkā 852, Añña 549.
Re-running replaces the `cst` rows in place, so it is idempotent and leaves the
8,762 `sc` rows untouched.

**The 1:1 cross-check passes: 217 source files on disk, 217 reachable from the
toc files** — zero either way. `toc1.xml` is read for its `src=` targets only
(61 of the 217) and is *not* built as a fourth root; its Mūla texts are browsed
on the SC spine. Zero missing `src=` files, zero duplicate node uids, zero
untitled nodes.

**1,920 toc leaves have no division record yet, and are written with
`is_available = 0`** — the expected state before task 4, which is what produces
the division records. They are reported per leaf, never pointed at the
whole-file record (that would make all twelve leaves of `s0305m.mul.xml` open
the same text). **24 leaves already resolve**: single-division files whose one
toc leaf names the source file itself (`e1215n.nrf.xml`, `s0514a2.att.xml`, …),
so the existing whole-file record *is* the division. Success metric 3's zero is
task 4's to reach.

**Three findings worth keeping:**

- *Depth must be counted independently of the nikāya structure.* The first
  version incremented the level index only while a `NikayaStructure` was in
  scope, so the whole Vinaya, Abhidhamma and Añña subtrees were labelled
  `group` at every depth. Depth is now the node's own level index, and the
  structure only decides which *name* that index gets.
- *The commentary tocs are shallower than the mūla structure they comment on,
  so a depth index alone mislabels them.* The Aṅguttara aṭṭhakathā's leaves are
  vaggas (`1. Rūpādivaggavaṇṇanā`) sitting at the depth `NikayaStructure`
  reserves for a paṇṇāsaka, and the Dhammapada aṭṭhakathā's are vaggas
  (`1. Yamakavaggo`) at a sutta's depth. `level_label` therefore reads the
  node's **own title** first (`saṃyutt` / `paṇṇāsa` / `vagg` / `nikāy` / `sutt`,
  matched on the stem), and falls back to the level list only for a title that
  names no level — `Ganthārambhakathā` and its like. The vocabulary is still
  `GroupType`'s, never a parallel one (FR-5), and this is reporting the source's
  own word rather than reconciling the two spines (T-2). Final labels: 1,074
  `division`, 489 `vagga`, 261 `sutta`, 128 `book`, 120 `samyutta`, 23
  `pannasaka`, 17 `nikaya`, 13 `group`, 3 `root`.
- *`toc1`–`toc4` are UTF-8 with a BOM although they declare `encoding="UTF-16"`,
  while the `cscd` tocs really are UTF-16.* `read_xml_file` detects by BOM and
  handles both; `reads_both_toc_encodings` asserts it against the real files
  (and skips when the checkout is absent).

**Añña (T-6) needed no deviation.** `toc4.xml` is properly structured, not a
flat grab bag: 9 titled groups (Visuddhimagga 5, Saṅgāyana-puccha vissajjanā 7,
Leḍī sayāḍo 4, Buddha-vandanā 8, Vaṃsa 3, Byākaraṇa 13, Nīti 10, Pakiṇṇaka 3,
Sihaḷa 15), each holding `src=` books whose leaves are divisions. The `cscd`
tocs are flat in all 217 files — one root `<tree>` with leaves under it — which
is the source's own shape, not a structure the builder failed to derive; the
leaf walk recurses anyway so a nested one would build rather than truncate.

**Vinaya (T-4) behaves as the PRD expects.** `vin02a1.att.xml` is
`cst:vin02a1.att` under Aṭṭhakathā › Vinayapiṭaka, `vin11t.nrf.xml` is
`cst:vin11t.nrf` under Ṭīkā › Vinayapiṭaka, and `vin02m1.mul.xml` is on the SC
spine — three roots, three internal structures, **no** cross-root join built.

### 4.0 CST coverage classes, division records, dual-identity rows, and the coverage report

> **Specs to keep in mind.**
> **The governing rule (§5.3, FR-14):** a coarser record is kept only where the
> finer ones do not cover the file, so the same text is never stored twice.
> Duplication is a search defect before it is a size problem — the whole-file
> records are **74 MB of the `suttas` table's 163 MB of `content_plain`**,
> indexed by a *trigram* FTS5 table and by an 841 MB Tantivy index, and a
> duplicated CST text returns two hits for every query that matches it.
> **The four classes and what each file gets:** fully mapped → fragments only;
> partially mapped → divisions **plus** its mapped fragments; unmapped →
> divisions only; unsplittable → the whole-file record as the FR-22 fallback.
> **The fully-mapped predicate is "every Sutta fragment has an `sc_code` **and**
> the fragments are gapless"** — computed per file, never a hard-coded list.
> **The shape of the work:** 217 source XML files in `romn/`, 217 `*.toc.xml` in
> `tipitaka.org/romn/cscd/` (1:1). The split rule is settled by measurement:
> `rend="chapter"` plus a leading division, 216 of 217 exact.
> **Dual identity (FR-16, FR-17):** in the *partially mapped* class only, the
> division rows and the mapped fragment rows are **sibling rows** under one
> parent, each independently openable, with the fragment visibly marked as a
> portion of its file.
> **Depends on:** 3.0 (the CST spine that these rows hang from), and
> `cli/src/bootstrap/tipitaka_xml.rs`'s existing import.

- [x] 4.0 CST coverage classes, division records, dual-identity rows, and the coverage report
  - [x] 4.1 ~~**Replace `CHECKED_XML_WHITELIST` with a computed coverage class.**~~
        **Done, then reverted — the premise was wrong.** The list is a *verification*
        gate, not a coverage predicate; it is kept (as `VERIFIED_CST_FILES`) and the
        computed class applies only within it. See the notes below.
        Derive each file's class from `fragments.sqlite3` — fully mapped only if
        **every** `frag_type='Sutta'` fragment carries an `sc_code` *and* the
        fragments leave no interior gap in the file's line span (FR-14, FR-15).
        A weaker "has any `sc_code`" predicate is a **data-loss defect**:
        `s0401m.mul.xml` has 2 of 33 mapped, and promoting it would keep 2
        suttas and lose 31. Re-run the import and report the class counts.
  - [x] 4.2 Confirm the reclassification against the measured baseline before
        trusting it: **28** files fully mapped, **22** partially, **167**
        unmapped. The four whitelist entries that are *partial* —
        `s0101a.att.xml` / `s0101t.tik.xml` (13 of 14) and `s0201a.att.xml` /
        `s0201t.tik.xml` (50 of 51) — must move to the partially-mapped class,
        which recovers the four suttas Phase 1 silently drops today. The two
        fully-mapped files missing from the whitelist, `s0305a.att.xml` (230)
        and `s0305m.mul.xml` (605), must move into fragment import, which makes
        835 already-mapped suttas reachable.
  - [x] 4.3 Implement the split in `cli/src/bootstrap/tipitaka_xml.rs` using the
        romn `rend="chapter"` markup **plus a leading division where body text
        precedes the first chapter**, producing one record per toc leaf named to
        line up with it (`s0305m.mul11.xml/pli/cst`) (FR-20). The method is
        already chosen by measurement — record the reasoning in a comment rather
        than re-deciding it. Define "body text" explicitly: a head of only
        `rend="nikaya"` / `"book"` / `"title"` / `"centre"` paragraphs is **not**
        a division.
  - [x] 4.4 **Drop the whole-file record wherever a finer partition covers the
        file** — for the fully-mapped class (fragments cover it) and for every
        file the split succeeded on (divisions cover it). Keeping both is the
        74 MB duplication FR-14 exists to prevent.
  - [x] 4.5 Implement the per-file cross-check: divisions produced must equal toc
        leaves for that file; report every file where they disagree (FR-20, FR-8)
        and do not silently accept either count. Expect exactly one today —
        `abh05t.nrf.xml` (5 leaves, 6 chapters, plus a leading head).
  - [x] 4.6 Keep the whole-file `…xml/pli/cst` import as the fallback for any file
        the split could not partition, so no text is ever lost (FR-22); flag it
        in the report rather than blocking the import.
  - [x] 4.7 Emit division nodes as leaves labelled with the toc leaf's `text=`
        title under their toc parent (FR-19), carrying `cst_file` so the row can
        say which source file it came from.
  - [x] 4.8 For the **fully mapped** class, emit each toc leaf as a **branch**
        whose children are that leaf's fragments (FR-18). Join on the fragment's
        group level — verified exact for `s0305m.mul.xml`, whose twelve toc leaf
        titles are byte-identical and in the same order to the distinct
        `json_extract(group_levels,'$[2].title')` values — falling back to
        `start_line` ordering. Report a leaf matching no fragment as a defect
        (FR-8) rather than guessing.
  - [x] 4.9 For the **partially mapped** class, emit each mapped fragment as a
        **sibling row** after its division row, in source order, with a field the
        QML can use to render "in `s0201a.att.xml`" (FR-16, FR-17). Fragments are
        never nested inside the division row.
  - [x] 4.10 ~~Confirm FR-15 by construction~~ — **FR-15 is not implementable as
        written**; adding *mappings* to an already-verified file still reclassifies it
        with no code change, but admitting a new file stays a deliberate act: the class is computed and the
        fragment rows are produced by querying which mapped uids exist, so adding
        mappings and re-running the bootstrap changes only which class a file
        falls in — no code path enumerates files.
  - [x] 4.11 Run the full bootstrap; record the FR-21 counts and the class counts
        as the baseline, confirm the `suttas` row count and total `content_plain`
        moved the way the classes predict (no net duplication), and confirm every
        one of the three CST roots reaches an openable leaf (success metric 2;
        it said "four" including Mūla, now corrected in the PRD — `toc1.xml` is
        not built as a root).
  - [x] 4.12 Create `coverage_report.rs` emitting: every toc leaf not assigned to
        a record or a populated branch; every `romn/` file unreachable from any
        toc leaf; each file's coverage class with mapped/total fragment counts;
        and **counts for all of them**, so a regression is a number (FR-21).
  - [x] 4.13 Add the `ANALYZE` call on appdata after the tree tables are
        populated (FR-12) and add the new write path to the table in
        `docs/user-data-and-sqlite-analyze.md`.

#### Implementation notes — 4.0 (coverage classes, divisions, coverage report), measured 2026-09-19

Two new CLI commands make this runnable without a full bootstrap, and they are
what every figure below was measured with:

``` sh
simsapa_cli import-cst --db-path <appdata> --cst-xml-dir <tipitaka-xml> \
    --fragments-db-path <fragments.sqlite3>
simsapa_cli build-navigation-tree --db-path <appdata> --cst-xml-dir <tipitaka-xml> \
    --fragments-db-path <fragments.sqlite3>
simsapa_cli index rebuild --area suttas --lang pli --simsapa-dir <SIMSAPA_DIR>
```

`import-cst` deletes the existing `%/pli/cst` rows first, because the importer
skips a uid that is already there and a re-run would otherwise change nothing.
**All three must be run together** — see the index note below.

##### The whitelist is a verification gate, and FR-15 is wrong about it

**`CHECKED_XML_WHITELIST` is kept** (renamed `VERIFIED_CST_FILES`, commented,
and moved to `cst_coverage.rs`). The PRD's premise — that it was a botched
attempt at a coverage predicate, and that computing the class would replace it
(FR-15, success metric 6) — **is incorrect, and implementing FR-15 literally
was a data-quality defect that had to be reverted.**

`tipitaka-xml-parser` is a work in progress that produces **wrong splits and
wrong uid assignments** on files whose structure it does not yet handle. The
list records which files a human verified; it says nothing about coverage.
Replacing it with "every fragment has an `sc_code` and the spans are gapless"
admitted **3,055 fragments from 20 unverified files** into the shipped app — a
confidently wrong parse satisfies that predicate as easily as a correct one.

The verification cannot be recovered from `fragments.sqlite3`, measured over
the shipped database:

- **`frag_review` is not a verdict.** `checked` means a human eyeballed that
  fragment so the parser could move past a stuck or edge-case state; its
  *absence* means the fragment never needed scrutiny, not that it is wrong.
  Only **155 of 7,349** Sutta fragments carry any review value (142 `checked`,
  13 `moved`), across 23 files, and **no file** has all of its fragments marked
  — not even a verified one, where 92 of 3,178 are marked. Two unverified files
  carry more marks than most verified ones (`s0305a.att.xml`: 47 `checked`;
  `s0402m1.mul.xml`: 9 `moved`, which reads like repair in progress).
- **The quality signal lives in the parser.**
  `tipitaka-xml-parser/src/web/validation.rs` runs nine checks over the parsed
  fragments — `missing_sc_code`, `unattached_text`, `missing_sc_sutta`,
  `sutta_review_status`, `xml_reconstruction`, `sc_code_sequence`,
  `code_uniqueness`, `cst_sc_range_consistency`, `sc_code_not_in_arangodb` —
  and reports per file whether its fragments are sound. **That verdict is never
  written back into the fragments database**, so the list is the only record of
  it. Adding an entry means running those validations, reviewing the output,
  then re-running the three commands above.

**The two name-spaces correlate by construction, so no mapping table was
needed.** The list names the files the *parser* read; the navigation tree works
in the *division* files the CST toc links (`s0101a.att0.xml`, …). Measured: all
30 entries appear in `cst_file`, in `romn/` and in the toc set, with **zero**
mismatches in either direction, and a source file's divisions are exactly the
leaves of its `<name>.toc.xml`. `toc_name_for()` is that derivation and is the
single place to replace if the naming ever diverges.

##### Result

| Class | Files | Records |
|---|---|---|
| fully mapped (verified) | **26** | fragments only |
| partially mapped (verified) | **4** | divisions + mapped fragments |
| unverified | **186** | divisions only |
| unsplittable | **1** (`abh05t.nrf.xml`) | the whole-file record |

**5,623 CST rows: 3,174 fragment, 2,448 division, 1 whole-file** (was 3,361 —
187 whole-file + 3,174 fragments). The fragment count is **identical to the old
whitelist output**: no unverified fragment reaches the app. The whole change is
that the 187 whole-file rows became 2,448 divisions cut at the CST toc's own
chapter boundaries — which do **not** depend on the parser's sutta splitting at
all, so they carry no verification risk. `content_plain` over the CST rows:
93.3 MB → 97.1 MB, the +3.8 MB being the accepted duplication of the 4
verified-but-partial files. `romn/` ↔ toc pairing 217/217 both ways.

The one genuine gain from the class computation survives: a **verified but
partially mapped** file (`s0101a.att.xml` 13/14, `s0201a.att.xml` 50/51 and
their ṭīkās) now gets divisions covering the remainder, which recovers the four
suttas the old phase 1 silently dropped.

**Spine: 3,916 nodes** (Aṭṭhakathā 1,620, Ṭīkā 1,747, Añña 549), **1,944 toc
leaves, 2 unassigned**. Shapes verified in the database: a fully mapped file's
toc leaf is a **branch** over its fragments; a partially mapped file's
divisions and fragments are **siblings** in source order, each carrying
`cst_file` for the FR-17 affordance; `abh05t.nrf.xml` has one whole-file row.

##### Findings worth keeping

- *The leading division cannot be read from the markup alone.* Deciding it by
  "body text precedes the first `rend="chapter"`" matches only **205** of 217
  files, whatever the definition of body text: eleven files open with real
  preamble gāthās (`s0508m.mul.xml`, `e0201n.nrf.xml`, `s0401a.att.xml` …)
  that the toc **folds into the first division** rather than listing. So the
  chapter markup gives the boundaries and the **toc leaf count decides**
  whether the head is its own division — 169 files at leaves = chapters, 47 at
  chapters + 1, `abh05t.nrf.xml` alone at chapters − 1. That is **216 of 217**,
  the PRD's figure, reached by a different route than the PRD describes. When
  the counts are equal the head is prepended to division 0, never dropped.
- *Several fragments can share one `sc_code`.* 110 of them do (three in
  `an4.1.att`), and the import keeps only the first — so a tree built per
  *fragment* named records that were never written and collided on `node_uid`,
  losing 110 subtrees. `load_fragments` de-duplicates by uid in the import's
  own order. (With the verification gate restored this affects far fewer rows,
  but the guard stays: it is a property of the data, not of the file set.)
- *A toc leaf with no fragments of its own opens the adjacent record, and that
  is correct.* SuttaCentral has no uid for a preamble or postamble section, so
  the corpus merges such a section into the first or last sutta record of the
  file **by design**. `s0301a.att.xml` and `s0301t.tik.xml` list
  `Ganthārambhakathā` as their first toc leaf, and the `sn1.1` fragment begins
  with that section before running on into the sn1.1 commentary — so the leaf
  opens `sn1.1.att/pli/cst`, which is where the text is. An earlier revision
  treated these as defects and dimmed them; they are now counted separately as
  "merged into an adjacent record". **Success metric 3 is 0 unassigned**, and
  the CST spine has zero unavailable nodes. A leaf with no fragments in a file
  that has none at all is still reported.
- *The join is on titles, not on a fixed group-level index.* A fragment falls
  under the leaf whose title matches its own `cst_sutta` or any of its group
  levels' titles (niggahita-folded). Matching group levels alone misses `dn2`
  and `dn13`, whose sutta-level group title is empty.

##### Replacing the CST records invalidates two things; only one heals itself

`suttas_fts` is kept in sync by its triggers, so ContainsMatch found the new
division rows immediately. The **Tantivy index does not** — it held
`s0404a.att.xml/pli/cst`, a uid that no longer exists — so
`index rebuild --area suttas --lang pli` is part of this change (12,911 Pāli
documents). **Any future re-run of `import-cst` needs the same rebuild.**

Two backend tests had to be updated, neither a behaviour regression:

- `test_search_normalization.rs` pinned the removed whole-file uid; it now
  asserts on `s0404a.att25.xml/pli/cst`, the division carrying that text.
- `test_fulltext_search_results.rs`'s fixture pinned **BM25 scores**, which
  depend on corpus-wide statistics and move whenever the corpus is re-cut.
  Regenerated with the file's own `generate_fulltext_fixture` ignored test. The
  10 result **uids and their order are byte-identical** and the total is still
  2,330.

#### Review corrections — applied 2026-09-19, after reviewing tasks 1.0–4.0

Four defects found by reading the built tree back out of the shipped
`appdata.sqlite3`. All four are in task 1–4 code and were fixed before task 5
started, because tasks 5 and 7 build directly on top of them.

**1. The root level interleaved the two spines (FR-6, FR-7).** Each spine is
built and written independently and numbered its roots from zero, so
`children_of(None)` returned `sutta(0) atthakatha(0) vinaya(1) tika(1)
abhidhamma(2) anna(2)` — the two groups shuffled together, with the tie broken
only by whichever spine was inserted last, so rebuilding one spine could change
the top-level order. `root_sort_base()` in `navigation_tree/mod.rs` offsets the
CST roots to 100. Now `sutta / vinaya / abhidhamma` then
`Aṭṭhakathā / Ṭīkā / Añña`, stable under an independent rebuild of either spine.

**2. The whole-tree filter was accent- and case-blind (FR-27).** It matched the
title columns with a bare `LIKE`, and SQLite folds case for ASCII only and does
no accent folding at all. Measured before the fix: `samyutta` → 5 hits,
`saṁyutta` → 132, `saccasamyutta` → **0** (the node is `Saccasaṁyutta`),
`MAHAVAGGA` → **0**. The PRD's own "type `dhammacakka`" user story worked only
because that word carries no diacritics.

Fixed with a folded `search_text` column (migration, `build_search_text`) over
the Pāli title, English title, `node_uid` and `sutta_uid`, and
`normalize_for_search` (`consistent_niggahita` → `pali_to_ascii` → lowercase)
applied to **both** the column and the query — one function, because a one-sided
fold is a silent miss rather than an error. After: all four spellings converge
(`samyutta` = `saṁyutta` = 199, `saccasamyutta` = 3, `MAHAVAGGA` = 25).

Titles are stored folded too, by `fold_title` in the single `flatten_node` pass
both spines share. The CST toc writes `Saccasaṃyuttaṃ` and SuttaCentral writes
`Saccasaṁyutta`, so the two spines were displaying one word two ways; 0 titles
now carry `ṃ` or `ŋ`. Blurbs are deliberately left alone — prose, shown as
written.

**3. SC leaves opened the CST edition for 1,379 texts (undefined by the PRD).**
`load_sutta_rows` kept the first Pāli row in uid order, and `dn1/pli/cst` sorts
before `dn1/pli/ms`, so browsing DN gave the CST text while browsing SN gave the
Mahāsaṅgīti — decided by alphabetical accident. `edition_rank()` makes it
explicit: `ms` → `cst` → other Pāli → non-Pāli. Now 7,283 `ms`, **2** `cst`
(`sn12.72-81`, `sn17.13-20` — ranged uids with no `ms` row) and 1 English
(a node with no Pāli text at all, opened rather than dimmed).

  Consequence, checked rather than assumed: the 1,379 CST mūla fragments those
  leaves used to open are no longer reached by *browsing*. They are reached the
  way every other alternative edition is — **the reader's translations panel**.
  `get_translations_data_json_for_sutta_uid` lists every `<ref>/%` row beside the
  opened one, so `dn1/pli/cst` is offered for an opened `dn1/pli/ms`, **when the
  user has enabled CST mūla** (`include_cst_mula`; the panel filters those rows
  out otherwise). Search reaches them either way. The tree gives one route to a
  text; the reader gives the editions of it — so do **not** add a second
  `sutta_uid` per node or a per-edition branch to reproduce that in the tree.

**4. Stale figure corrected.** The 4.0 notes said "1,944 toc leaves, 2
unassigned" two paragraphs above "Success metric 3 is 0 unassigned". The report
prints **0 unassigned, 2 merged into an adjacent record**; the 2 are
`s0301a.att0.xml` and `s0301t.tik0.xml`, the expected preamble case the notes
describe. Success metric 3 is met.

**Not changed, and why.**

- *The 661 Mūla division records unreachable from the tree are correct.*
  `toc1.xml` is not built as a fourth root (FR-6), so Mūla divisions have no
  browse path. They exist so no text is **missing** while
  `tipitaka-xml-parser` catches up: 30 of 217 files are verified today, and an
  unverified file's suttas have no mapped uid, so its division records are what
  keep the text present and findable by search. As more files are verified their
  suttas become mapped records on the SC spine. A Mūla root would be a second
  browsing path to material the SC spine already covers.
  **Success metric 2 named four CST roots including Mūla and has been corrected
  in the PRD** — three CST roots is the design. Written up as
  `docs/sutta-navigation-tree.md` §1.1.
- *56 of the 58 single-child toc-leaf branches are now openable leaves.* Where
  a fully mapped file's toc leaf joins to exactly one fragment **and** the two
  titles match, the leaf carries the fragment's `sutta_uid` directly and has no
  child — the child was repeating its parent verbatim
  (`1. Mahāpadānasuttavaṇṇanā` containing one row also reading
  `1. Mahāpadānasuttavaṇṇanā`) and cost a tap for nothing.
  **No source level is skipped**: the level dropped is not in the CST toc, it is
  one the fragment join invents, and a one-element group is not a level. The
  equivalence test is what keeps that honest — the 2 leaves whose single
  fragment has a different name (`4. Okkantasaṁyuttaṁ` over
  `1-10. Cakkhusuttādivaṇṇanā`, aṭṭhakathā and ṭīkā) are real containments and
  keep their branch. CST spine 3,916 → 3,860 nodes, 0 unavailable, 0 unassigned
  toc leaves, 0 dangling `sutta_uid`s. The other 212 single-child branches are
  genuine source levels (`sutta/long` → `dn`) and are untouched.
- *Success metric 1's interaction budget was withdrawn, not missed.* `sn56.11`
  is `sutta › sutta/linked › sn › sn-mahavaggasamyutta › sn56 ›
  sn56-dhammacakkappavattanavagga › sn56.11` — six enter-level presses plus
  Open. The metric demanded "five or fewer" while enumerating a seven-step chain
  in the same sentence, so it contradicted itself; the PRD now carries the
  correction.
  **Making the sectioning visible is the point of the window** — those levels are
  precisely what search-only cannot show — so no level may be collapsed to
  reduce a count. The fast path is the filter, which matches level names as well
  as sutta titles (`the_filter_matches_level_names_as_well_as_suttas`), putting a
  known section one entry away. Metric 2's "four CST roots" was corrected to
  three in the same pass.
- *A minor version bump is required before release and is the user's to make.*
  The nav tables are new and empty for an existing install, and nothing
  populates them at runtime (non-goal 1 rules out an in-app build), so the
  feature only reaches a user who downloads a fresh `appdata.sqlite3`. That
  needs `major.minor` to differ from the installed DB's
  (`is_app_version_compatible_with_db_version`). Four sites:
  `bridges/Cargo.toml`, `backend/Cargo.toml`, `cpp/gui.cpp`
  (`setApplicationVersion`) and `cli/src/bootstrap/appdata.rs` (`DB_VERSION`).
  The migration's "no DB version bump and no re-download" comment refers to the
  *table*, not the data, and should not be read as saying otherwise.

Verification: `cargo test` 623 backend lib (16 navigation_tree, 3 of them new
folding/level-name regressions) + all backend integration tests + 91 CLI, all
passing; both spines rebuilt against the real `appdata.sqlite3` and every figure
above read back out of it.

### 5.0 Rust bridge API for the tree, plus `qmllint` stubs

> **Specs to keep in mind.**
> **Bridge surface** (JSON strings, as the topic-index functions do):
> `nav_tree_is_available() -> bool`,
> `nav_tree_children_json(parent_uid: QString) -> QString` (empty string = root
> level), `nav_tree_filter_json(query, limit) -> QString`,
> `nav_tree_node_json(node_uid) -> QString`,
> `open_sutta_navigation_window()` (mirrors `open_topic_index_window()` at
> `bridges/src/sutta_bridge.rs:5802`).
> **Threading:** a level query is small and synchronous; anything that must run
> off-thread queues back with `crate::queue_or_log()` — never `.unwrap()`, never
> `let _ =` (FR-52).
> **Depends on:** 1.0 (read layer). Data from 2–4 is not required to build this,
> only to make it return rows.

- [x] 5.0 Rust bridge API for the tree, plus `qmllint` stubs
  - [x] 5.1 Read `SuttaBridge::load_topic_index` / `get_topic_index_letters`
        (`bridges/src/sutta_bridge.rs:5710`, `:5730`) as the precedent for
        function shape, JSON serialization and signal use.
  - [x] 5.2 Add the `nav_tree_*` functions to the `#[cxx_qt::bridge]` block and
        implement them over `simsapa_backend::navigation_tree`.
  - [x] 5.3 Serialize nodes with every field the row rendering needs:
        `node_uid`, `title_pali`, `title_en`, `blurb`, `sutta_uid`, `cst_file`,
        `node_type`, `tree_source`, `has_children`, child count, an
        `is_openable` flag (FR-42) and the fragment's parent-file label (FR-17).
  - [x] 5.4 Make the unavailable case explicit: when `nav_tree_is_available()` is
        false the window shows "navigation index not available", never an empty
        tree (FR-13).
  - [x] 5.5 Add `open_sutta_navigation_window()` plus its FFI callback, following
        `open_topic_index_window()`.
  - [x] 5.6 Add matching stubs with correct signatures and trivial return values
        to `bridges/assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` (T-7).
  - [x] 5.7 Use `crate::queue_or_log()` for any worker that queues back to the Qt
        thread (FR-52); add none that `.unwrap()`s.
  - [x] 5.8 `make build -B`, `cd backend && cargo test`, `make qml-lint`.

Notes:

- **The JSON shape lives in the backend, not in the bridge.**
  `NavigationNodeView` / `NavigationMatchView` in `backend/src/navigation_tree.rs`
  derive the three flags the row rendering needs -- `is_openable` (FR-42),
  `is_fragment` and `in_file` (FR-17) -- so they are unit-testable without a Qt
  object. The bridge functions are one line each.
- **A division row and a fragment row carry the same `cst_file`** (they are
  siblings under one parent, FR-16), so the file name cannot tell them apart.
  The discriminator is the `cst:frag:` uid prefix, now the shared constant
  `navigation_tree::FRAGMENT_UID_PREFIX` written by `cst_spine.rs` and read back
  by `NavigationNode::is_fragment()` -- previously a literal in the CLI only.
  `in_file` is `None` on a division so only the fragment gets the subdued label.
- **`is_openable` is two conditions, not one:** a `sutta_uid` *and*
  `is_available`. A leaf whose target text was not found when the spine was
  built keeps its uid (so the row can say what it would have opened) but is
  dimmed.
- **`nav_tree_filter_json` refuses a non-positive limit.** SQLite reads a
  negative `LIMIT` as "no limit", so a QML slip would return the whole tree as a
  flat list rather than an error.
- **Nothing here runs off the Qt thread**, so 5.7 adds no `queue_or_log()` call:
  a level is one indexed `WHERE parent_uid = ?` and the filter is two statements
  over materialized columns. No `.unwrap()` and no `let _ =` was added.
- **`callback_open_sutta_navigation_window()` is declared and wired end to end,
  but `cpp/gui.cpp` logs a placeholder** -- `WindowManager::create_sutta_navigation_window()`
  arrives in 6.3, which replaces that body. Logging rather than doing nothing
  keeps an early wiring mistake visible in `log.txt` instead of looking like a
  dead menu item.

Verification: `make build -B` green; `cargo test` 633 backend lib (26
navigation_tree, 7 of them new view-layer tests) + all backend integration tests
+ 97 CLI, all passing; `make qml-lint` reports no warning naming
`SuttaBridge.qml`.

### 6.0 Window lifecycle: wrapper, `WindowManager` hybrid family, entry points

> **Specs to keep in mind.**
> **This window is a third lifecycle family** — one instance, **hidden** on
> close, reuse predicate **`m_root != nullptr`** (not `visible`), null-`m_root`
> eviction still applies, revive does **not** clear state, excluded from session
> save/restore and from the `window_id`-less dispatch fallbacks (FR-23 to FR-25,
> resolved question 6).
> **Existing machinery to reuse:** `reuse_or_evict<T>()`
> (`cpp/window_manager.cpp:643`) already implements the predicate and the
> eviction; `show_and_activate_window()` is the only permitted raise;
> `save_session_now()` (`:444`) iterates `sutta_search_windows` only.
> **Depends on:** 5.5 (the bridge entry point).

- [x] 6.0 Window lifecycle: `SuttaNavigationWindow` C++ wrapper, `WindowManager` hybrid family, entry points
  - [x] 6.1 Read `docs/window-lifecycle-and-reuse.md` §0 and §7 before writing
        any of this, and note which of §7's steps do **not** apply to a
        hide-on-close window.
  - [x] 6.2 Create `cpp/sutta_navigation_window.{h,cpp}` modelled on
        `cpp/topic_index_window.{h,cpp}`, guarding
        `m_root = rootObjects().isEmpty() ? nullptr : …constFirst();`.
  - [x] 6.3 Add `sutta_navigation_windows` to `WindowManager` and
        `create_sutta_navigation_window()` using `reuse_or_evict()` +
        `show_and_activate_window()` — and **no** `clear_all_tabs()` analogue,
        since preserving state is the whole point (FR-24).
  - [x] 6.4 Make the QML `onClosing` **accept** the close and simply **not**
        route to `notify_window_closed` / `on_window_closed` — the wrapper is
        never destroyed, so there is no destruction to defer (FR-24, T-8). Do
        **not** reject the close or assign `visible = false`:
        `maybeLastWindowClosed()` runs only inside `case QEvent::Close` and only
        when the event was accepted (`qwindow.cpp:2700-2716`), so rejecting it
        is what would leave a live app with no windows. Accepting it makes Qt
        call `destroy()`, which releases the platform window but leaves
        `m_root`, the engine and all QML state intact.
  - [x] 6.5 Read `save_session_now()` and the startup restore and **confirm by
        reading** that neither can see the new window; if either could, exclude
        it explicitly (FR-25, resolved question 6).
  - [x] 6.6 Read `last_open_sutta_search_window()` /
        `first_open_sutta_search_window()` and confirm they remain
        sutta-window-only (FR-24).
  - [x] 6.7 Add the menu-bar item to `SuttaSearchWindow.qml` beside
        `&Topic Index...` and the drawer-menu entry for mobile (FR-26).
  - [x] 6.8 Route both entry points through the one `WindowManager` accessor, so
        the single-instance guarantee holds regardless of which
        `SuttaSearchWindow` asked (FR-23, FR-26).
  - [x] 6.9 Verify the last-window case on the built app: nothing in `cpp/` sets
        `quitOnLastWindowClosed(false)`, and with 6.4's accepted close the quit
        check does run, so closing the navigation window as the only visible
        window should quit the app exactly as closing a `SuttaSearchWindow`
        does. Confirm that, and confirm reopening after a close restores the
        prior level rather than a fresh window.
  - [x] 6.10 `make build -B`; the window opens empty and closes without crashing.

Notes:

- **Which of §7's checklist steps do not apply to a hide-on-close window** (6.1):
  step 4's `on_window_closed()` branch (nothing is ever destroyed, so a request
  naming this window falls through to the unknown-type error, where a wiring
  mistake is visible in `log.txt`); step 5's `notify_window_closed()` call; step
  6's §5b deferral (there is no destruction to defer, so no `close_pending`
  flag, no failsafe `Timer` and no reopen-cancels-the-close trap); and step 6a's
  re-init on show, which is **inverted** here -- preserving what
  `Component.onCompleted` left is the requirement, not a hazard. Steps 1, 2, 3,
  7 and 8 apply unchanged. The one thing 6a still flags for later: the
  translation counts of FR-47 *are* time-varying, so task 8 must decide where
  they refresh rather than assuming the level is still current after a language
  download.
- **6.5 and 6.6, confirmed by reading, and no explicit exclusion was needed.**
  `save_session_now()` and `restore_last_session()` both iterate
  `this->sutta_search_windows` alone, as do
  `last_open_sutta_search_window()` / `first_open_sutta_search_window()`. The
  new window lives in its own `sutta_navigation_windows` list, so it is
  invisible to all four by construction -- the session array's length stays the
  sutta-window count, and a `window_id`-less dispatch cannot land on it.
- **One `CMenuItem` serves both entry points** (6.7, 6.8). `DrawerMenu.qml`
  builds the mobile menu by walking the same `menu_list` the desktop menu bar
  renders, and `windows_menu` is in that list -- so the single
  `action_sutta_navigation` Action is the mobile route as well, and both go
  through `SuttaBridge.open_sutta_navigation_window()` to the one
  `WindowManager` accessor.
- **The `gui.cpp` placeholder from task 5 is gone**; the callback now calls
  `create_sutta_navigation_window()`.
- **The QML window is a placeholder body**: the root, the `onClosing` rule and
  the `nav_tree_is_available()` empty-state message. The filter bar, breadcrumbs
  and rows are task 7.
- **6.9's static half is confirmed:** nothing in `cpp/` or the QML tree calls
  `setQuitOnLastWindowClosed(false)` -- the only two mentions are comments in
  `storage_recovery_window.cpp` and `gui.cpp` about the startup windows -- so
  the default `true` holds and 6.4's accepted close does run the quit check.
  **The runtime half is for the user to check by hand** (GUI runs are not an
  agent task here): closing the navigation window as the only visible window
  should quit the app, and reopening after a close should revive the same
  window rather than build a fresh one -- `log.txt` says which, since the
  accessor logs `revived` vs `created`.

Verification: `make build -B` green; `make qml-lint` reports no warning naming
`SuttaNavigationWindow.qml` or `SuttaSearchWindow.qml`. Runtime behaviour
(6.9, 6.10) pending the user's check.

### 7.0 QML window: two row modes, breadcrumbs, drill-down, filter bar, blurb toggle

> **Specs to keep in mind.**
> **Layout:** filter input on top, breadcrumb bar below it, the level list under
> that. Entering a level resets indentation to zero, so the deepest level uses
> the full row width (FR-37).
> **The interaction rule (FR-32, corrected 2026-09-23): a tap on a row body does
> the one thing the row is for** — a branch expands or collapses, an openable
> leaf opens its text (the same as its Open button). Entering a level and
> expanding a blurb are **buttons** only. A row is never both a branch and a
> leaf, so there is still no ambiguity for a touch screen to resolve.
> **Two row modes (FR-33), decided by what the row *is*, not by a setting:** a
> **level row** is a direct child of the current breadcrumb level and gets the
> full FR-43 / FR-46 rendering including the blurb; a **tree row** is anything
> revealed by expanding, at any depth, and is compact — title, English title,
> uid, enter-level button, **no blurb ever**, whatever the FR-31 toggle says.
> Tree rows exist so a branch can be flicked open to check whether the
> sub-items are the ones being looked for, then flicked shut.
> **Precedents:** `ChantingTreeList.qml` for the expand/collapse tree and the PNG
> carets — `icons/32x32/fe--drop-down.png` / `fe--drop-right.png` at lines 243
> and 338, **never Unicode characters** (FR-36); `TopicIndexWindow.qml`
> for window layout and `extra_top_margin`.
> **Layout rules that are easy to break:** never assign `topPadding`/`padding` on
> the `ApplicationWindow` root; anchor the root content item to its parent rather
> than sizing it from `root.width`/`root.height`; any `Dialog` clamps its own
> width to `Overlay.overlay` and uses `header: DialogHeader { … }` if it has a
> title plus wrapping text.
> **Conventions:** files under `bridges/assets/qml/`, registered in
> `bridges/build.rs` as exactly `"assets/qml/<Name>.qml"`; `Logger { id: logger }`
> with single-string messages, never `console` (FR-49).
> **Depends on:** 5.0 (bridge), 6.0 (a window to put it in).

- [x] 7.0 QML window: two row modes, breadcrumbs, drill-down, filter bar, row components, blurb toggle
  - [x] 7.1 Create `SuttaNavigationWindow.qml` with the root layout, register it
        and every new component in `bridges/build.rs`'s `qml_files` using the
        exact `"assets/qml/<Name>.qml"` form (no `..`).
  - [x] 7.2 Implement the level model: a list populated from
        `nav_tree_children_json()`, replaced wholesale when the enter-level
        button is pressed (FR-34), with expansion state not carried across.
        Keep the query off the first-paint path (FR-50).
  - [x] 7.3 Implement `NavigationBreadcrumbBar.qml`: full path from root to the
        current level, pressing a segment returns to that level, middle segments
        elided on a narrow screen rather than wrapping (FR-35, FR-37a).
  - [x] 7.4 Make a tap on a branch row's body expand/collapse that node in
        place, revealing its children as **tree rows** (FR-32, FR-33). Show the
        state with the two PNG icons exactly as `ChantingTreeList.qml` does —
        the caret is an indicator, the whole row body is the tap target (FR-36).
  - [x] 7.4a Add the **enter-level button** to every branch row, in both modes:
        it makes that node the current level, resets indentation to zero and
        appends a breadcrumb segment (FR-34).
  - [x] 7.5 Implement the filter input: whole-tree filter via
        `nav_tree_filter_json()`, results as a **flat list** each carrying its
        breadcrumb path (FR-27); clearing it returns to the pre-filter level
        (FR-28); a result row has no body tap action and carries the same
        buttons as a tree row — **Open** on a leaf, **enter level** on a branch
        (FR-29).
  - [x] 7.6 Apply the mobile soft-keyboard rules to the filter input:
        `MobileKeyboardHelper`, `EnterKey.type: Qt.EnterKeySearch` with a
        matching `onAccepted`, `focus: root.is_desktop`, `Qt.ImhNoAutoUppercase`
        only (FR-30).
  - [x] 7.7 Add the blurb toggle to the top bar, shown on by default, persisted
        in `AppSettings` — the struct carries `#[serde(default)]` at the
        container level (`backend/src/app_settings.rs:186`), so a new field
        deserializes safely against existing stored settings (FR-31) — and only this setting; breadcrumbs, filter
        text, expansion and scroll are **not** persisted (FR-25).
  - [x] 7.8 Create `NavigationNodeRow.qml` with a `is_tree_row` property
        selecting the two modes. A leaf **level row** renders Pāli title →
        English title → uid (always visible, dimmer and smaller) → blurb →
        translation hint placeholder → **Open** button, in that order; a leaf
        **tree row** renders the first three plus the Open button and nothing
        else (FR-43, FR-33).
  - [x] 7.9 Render a branch **level row** with title, English title, uid,
        blurb, child-count indication, caret and enter-level button; a branch
        **tree row** with title, English title, uid, caret and enter-level
        button only (FR-46, FR-33).
  - [x] 7.10 Render **no blurb area at all** when there is no blurb — no reserved
        space, no placeholder, varying row heights accepted (FR-44).
  - [x] 7.11 Truncate the blurb to roughly two lines and add a **button**
        ("more" / "less") that expands and collapses that row's full blurb;
        expansion is per row, not a global mode. It must be a button, not a tap
        on the text: the row body's tap belongs to expand/collapse of the node
        (FR-45, FR-32).
  - [x] 7.12 Render a fragment row with its subdued "in `s0201a.att.xml`"
        affordance so it cannot be mistaken for a duplicate of the division row
        above it (FR-17).
  - [x] 7.13 Render a node whose target text is absent as dimmed with its Open
        button disabled (FR-42), and render the whole-window "navigation index
        not available" state when the tree is empty (FR-13).
  - [x] 7.14 `make qml-test`; confirm no new `qmllint` warning names a file you
        touched.

Notes:

- **The row mode is a property of the row, not of the window.**
  `NavigationNodeRow.qml` takes `is_tree_row` and `is_filter_row`; a level row
  is neither. The blurb is gated on `!is_tree_row && !is_filter_row &&
  show_blurbs`, so a tree row cannot show one however the toggle is set.
- **Blurb "more"/"less" state lives in the window, not in the delegate.** A
  `ListView` recycles delegates, so a per-delegate flag would land on whichever
  row scrolled into that delegate. `blurb_expanded_uids` is keyed by `node_uid`,
  as `expanded_uids` and `children_cache` already are.
- **Two `ListView`s, not one with a switched model.** The level list keeps its
  scroll position while the filter list is shown, which is what makes clearing
  the filter return to the level the user was on with no restore code (FR-28).
- **Entering a level from a filter hit needs uids the hit does not carry.**
  `NavigationMatchView.breadcrumbs` is ancestor *titles* — enough to display
  where a hit sits, not enough to navigate to it. `ancestor_segments()` walks
  `parent_uid` up through `nav_tree_node_json()`, guarded at 32 steps against a
  cycle in defective data.
- **The breadcrumb bar's `…` is a menu, not just an ellipsis.** Collapsing the
  middle segments to fit a phone screen would otherwise make those levels
  unreachable, which is the one thing the bar exists for (FR-35). The first
  segment and the tail stay visible; how many fit is derived from the bar's
  width rather than a fixed count.
- **The blurb toggle is the only persisted state** (FR-31 vs FR-25).
  `show_navigation_blurbs` was added to `AppSettings`, which carries
  `#[serde(default)]` at the container level, so stored settings from an older
  build deserialize with the `true` default rather than failing.
- **`Open` is rendered and gated here but does not yet open anything** — the
  row's `open_text()` signal reaches `open_node()`, which refuses an
  unavailable node and logs the uid. The dispatch to a `SuttaSearchWindow` tab
  is task 8.1, as is the translation count: `NavigationNodeRow` already carries
  `translation_count`, hidden while it is -1.
- **The first level query is deferred with `Qt.callLater`** so the window paints
  before the roots are read (FR-50).
- **Two defects found in review after the GUI check and fixed.** (1) FR-42's
  dim was applied to the row's `background` Rectangle only, leaving the title,
  uid and buttons at full strength -- it read as a style, not as "unavailable";
  the `opacity` is now on the row. Latent today: the built tree has **0** nodes
  with a `sutta_uid` but `is_available = 0`, so nothing exercises it yet, but a
  bootstrap that cannot resolve a leaf produces them by design (FR-8).
  (2) `append_expanded()` recursed without a depth cap, so a `parent_uid` cycle
  in defective data would overflow the stack -- a hard crash rather than a
  visible defect. Capped at 24 with a `logger.error`; the real maximum ancestor
  depth in the built tree is **7**.
- **The blurb toggle is an `property alias` to the checkbox, not a mirrored
  `property bool`.** A `CheckBox` assigns its own `checked` when clicked, which
  replaces any binding written onto it -- so `checked: root.show_blurbs` would
  silently stop tracking after the first click. The alias leaves one piece of
  state, matching `AppSettingsWindow.qml`'s idiom (imperative assignment at
  load, handler on change). `onToggled` rather than `onCheckedChanged`, because
  the latter also fires on the load-time assignment and would rewrite the
  setting on every window open.
- **Measured against the built tree:** 12,622 nodes, max ancestor depth 7,
  largest single level **728 rows**, 1,736 fragment rows (so FR-17's "in
  `<file>`" label renders) and 4,020 blurbs (so FR-31/44/45 render). The
  728-row level is what task 9.4 must time, and is why task 8's translation
  count has to be one query per level.

Verification: `make build -B` green; `cargo test` 633 backend lib + all backend
integration tests passing; `make qml-test` 172 passed, 0 failed; `make qml-lint`
reports no warning naming `SuttaNavigationWindow.qml`, `NavigationNodeRow.qml`,
`NavigationBreadcrumbBar.qml` or `SuttaBridge.qml`. Runtime behaviour is for the
user to check by hand.


---

### 8.0 Opening a text, and the installed-language translation count

> **Specs to keep in mind.**
> **Dispatch:** follow `TopicIndexWindow.qml`'s `open_sutta()` (line 390) — build
> `result_data` as `{ item_uid, table_name: "suttas", anchor }` and route it via
> `SuttaBridge.open_sutta_search_window_with_result()` or
> `emit_show_sutta_from_reference_search()` (FR-38).
> **Count semantics:** "translations installed right now", so it changes with
> language downloads and cannot be stored at bootstrap (FR-47). One query per
> displayed level, never per row (FR-48); any cache is invalidated by
> `AppData::refresh_language_caches()` (`backend/src/app_data.rs:243`).
> **Depends on:** 7.0 (rows to attach counts to), 5.0 (bridge).

- [x] 8.0 Opening a text, and the installed-language translation count
  - [x] 8.1 Implement leaf tap → open in a **new tab** of a `SuttaSearchWindow`
        following `TopicIndexWindow.open_sutta()` (FR-38).
  - [x] 8.2 Keep the navigation window open after opening a text, on desktop and
        mobile alike (FR-39).
  - [x] 8.3 Desktop: raise the sutta window via `show_and_activate_window()`,
        never a raw `show`/`raise`/`requestActivate` triple (FR-41).
  - [x] 8.4 Mobile: bring the sutta window to the front, leaving the navigation
        window hidden behind it with its state intact (FR-40).
  - [x] 8.5 Add a backend level-count query: for the `sutta_uid`s of one
        displayed level, count `suttas` rows in the installed languages in a
        single query (FR-48). Prefer the indexed prefix-range shape
        (`language IN (…) AND uid >= '<uid>/' AND uid < '<uid>0'`, unioned),
        which uses `idx_suttas_language_uid` and scales as languages are
        installed; the `substr` GROUP BY also measured fine (14 ms vs 9 ms) and
        is acceptable — `EXPLAIN QUERY PLAN` shows **both** shapes using that
        index as a *covering* index, so neither touches the 842 MB of content
        blobs. **Key the range on the bare uid** (`sn56.11`), not on the node's
        full `sutta_uid` (`sn56.11/pli/ms`); FR-9's column list has no bare-key
        column, so split at the first `/` or add one.
  - [x] 8.6 Expose it as one bridge call taking the level's uids and returning a
        uid → count map; call it once per level render, never per row.
  - [x] 8.7 **Do not add a cache** — both query shapes are ~10 ms against the
        real DB, and a cache only creates the stale-count defect FR-48 warns
        about. If measurement later forces one, invalidate it from
        `AppData::refresh_language_caches()` (`backend/src/app_data.rs:243`).
  - [x] 8.8 Render the count as the small indicator of FR-43 item 5.
  - [x] 8.9 `make build -B`, `cd backend && cargo test`, `make qml-test`.


Notes:

- **The dispatch is `emit_show_sutta_from_reference_search("", …)`**, the same
  route `TopicIndexWindow.open_sutta()` takes with `open_in_new_window` off:
  it opens a **new tab** in the last *visible* Sutta Search window. The
  navigation window is not involved in the raise and is never hidden by it, so
  FR-39 needs no code.
- **`show_sutta_from_reference_search()` used a raw `show`/`raise` pair**
  (`cpp/window_manager.cpp:1073`) rather than the shared
  `show_and_activate_window()` the rule in docs/window-lifecycle-and-reuse.md
  names. Changed, which satisfies FR-41 for this window and fixes the same
  omission for the two windows already on that route (Topic Index, Reference
  Search): the helper also clears `Qt::WindowMinimized` and sends
  `_NET_ACTIVE_WINDOW` on X11, which is what actually brings a window forward
  from another window's request. That single raise is also FR-40 on mobile.
- **With no Sutta Search window open, the route silently does nothing.**
  `last_open_sutta_search_window()` filters on `visible`, so a user who closed
  every sutta window would press Open and see no result at all. `open_node()`
  therefore checks `count_open_sutta_search_windows()` first and falls back to
  `open_sutta_search_window_with_result()`.
- **The count is a prefix range per uid, not a `LIKE`.** `uid >= 'sn56.11/'
  AND uid < 'sn56.110'` (`'0'` is the byte after `'/'`), OR-ed per uid and
  chunked at 100 because SQLite caps expression depth at 1000. `LIKE
  'sn56.11%'` would fold `sn56.110` into `sn56.11`; a regression test covers
  exactly that neighbour case.
- **Measured against the shipped `appdata.sqlite3`:** `EXPLAIN QUERY PLAN`
  reports `MULTI-INDEX OR` over `SEARCH suttas USING COVERING INDEX
  idx_suttas_language_uid`, so none of the content columns is read. The
  largest leaf level in the built tree is `patthana` at **728 rows**, answered
  in **13.2 ms** across 8 chunks; a 50-row `sn56` level takes **1.2 ms**.
- **No cache (8.7).** At those timings a cache buys nothing and creates the
  stale-count defect FR-48 warns about. The counts are re-read per level and
  again in `onVisibleChanged`, which is the case that actually matters: the
  window is hidden rather than destroyed, so a language downloaded while the
  user was reading would otherwise leave the old numbers on screen.
- **The root language is excluded.** A text is not a translation of itself, and
  a node's `sutta_uid` is almost always `…/pli/…`; without the exclusion every
  Pāli-only text would claim one translation. A uid with no translation is
  **absent** from the returned map rather than present as `0`, which is what
  the row already renders as "no indicator" (`translation_count > 0`).
- **Only level rows are counted.** `load_translation_counts()` collects uids
  from `level_nodes`, not from `display_rows`, so expanding a branch does not
  enlarge the query — tree rows never show the indicator anyway (FR-33).
- Counts are fetched with `Qt.callLater` after the level's rows are drawn, for
  the same reason the first level query is (FR-50).

Verification: `make build -B` green; `cargo test` 641 backend lib tests + all
integration suites passing (8 new count tests); `make qml-test` 172 passed, 0
failed; `make qml-lint` reports no warning naming `SuttaNavigationWindow.qml`
or `SuttaBridge.qml`. Runtime behaviour is for the user to check by hand.

---

### 9.0 Verification, success metrics, and documentation

> **Specs to keep in mind.** The success metrics are measurements to *report*,
> not claims to assert. Metric 3 (unassigned toc-leaf count = 0) is the FR-21
> report's first number; metrics 1, 4, 5 and 6 each need an explicit run.
> Device checks (metric 5, the `MobileOverlayTracker` check) must be handed to
> the user — do not drive the GUI.
> **Depends on:** all of 1.0–8.0.

- [ ] 9.0 Verification, success metrics, and documentation
  - [ ] 9.1 Run the full test suite (`make test`) and record the result.
  - [ ] 9.2 Walk from a cold window to `sn56.11`, report the path and the count
        (measured today: six enter-level presses plus Open), and confirm every
        section level on it is visible and named. **There is no interaction
        ceiling and no level may be collapsed to reduce the count** — metric 1's
        "five or fewer" was withdrawn, see the correction in the PRD. Making the
        sectioning visible is the point of the window; the filter (FR-27, which
        matches level names as well as sutta titles) is the fast path for a
        reader who already knows what they want.
  - [ ] 9.3 Report the FR-21 coverage counts and the per-class file counts; the
        unassigned toc-leaf count must be zero (**measured 0** at the end of task
        4, with 2 leaves counted as "merged into an adjacent record" and
        `abh05t.nrf.xml` on the FR-22 fallback), and the unreachable-file count is
        informational (metric 3).
  - [ ] 9.4 Measure level-render time for the largest level on desktop and report
        it against the 100 ms target (metric 4).
  - [ ] 9.5 Add a small batch of new CST→SC mappings, re-run the bootstrap, and
        confirm the tree contents change with **no** code change (metric 6,
        FR-15).
  - [ ] 9.6 **Device check, not a code change:** ask the user whether the sutta
        window's native webview bleeds through the navigation window on Android.
        `MobileOverlayTracker` cannot see a separate-engine top-level window and
        must not be extended to (see the Notes and
        `docs/mobile-webview-visibility-management.md`); if bleed-through is
        observed it is a pre-existing defect shared with every secondary window,
        and belongs in its own PRD.
  - [ ] 9.7 Ask the user to verify on a device that opening a text and returning
        via the menu restores the exact prior level, filter text and scroll
        position (metric 5, FR-40).
  - [ ] 9.8 **Extend** `docs/sutta-navigation-tree.md` — created during task 4
        with §1-§7 (the two spines, node-uid stability, `VERIFIED_CST_FILES` and
        why it cannot be computed, the coverage classes, the split rule and its
        traps, the coverage report, the rebuild commands). Still to add: the KN
        deviations from 2.9 and the Añña notes from 3.7, plus anything the
        window itself needs. Originally scoped as: the two spines and why they are
        not reconciled (T-2), the **four coverage classes and the rule that a
        coarser record is kept only where no finer partition covers the file**
        (§5.3, FR-14) together with the 74 MB / trigram-index reason it exists,
        the dual-identity rule for the partially mapped class, node-uid
        stability (T-3), the split method chosen and its cross-check (FR-20),
        and the KN / Añña deviations found in 2.9 and 3.7.
  - [ ] 9.9 Amend `docs/window-lifecycle-and-reuse.md`: add the third family to
        §0 with the reasoning from FR-23 to FR-25, and extend §5b's table with
        the fact that this window has no destruction to defer (T-8). **§5b-bis
        is already done** -- it carries the once-per-*app-run*
        `Component.onCompleted` consequence and the three values this window
        reads at load, including the accepted staleness of
        `get_mobile_extra_top_margin()` on mobile. Extend it rather than
        duplicating it.
  - [ ] 9.10 Confirm `docs/user-data-and-sqlite-analyze.md` carries the new write
        path (added in 4.10) and update `PROJECT_MAP.md` with the new files.
