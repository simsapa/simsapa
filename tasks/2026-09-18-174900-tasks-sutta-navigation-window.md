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
- `cli/src/bootstrap/navigation_tree/coverage_report.rs` — the FR-21 coverage report.
- `cpp/sutta_navigation_window.cpp` / `.h` — the window wrapper (FR-23, FR-24).
- `bridges/assets/qml/SuttaNavigationWindow.qml` — the window (FR-27 to FR-46).
- `bridges/assets/qml/NavigationNodeRow.qml` — leaf/branch row rendering (FR-43 to FR-46, FR-49).
- `bridges/assets/qml/NavigationBreadcrumbBar.qml` — breadcrumb bar with middle-segment elision (FR-35, FR-37a).
- `docs/sutta-navigation-tree.md` — the two spines, the dual-identity rule, node-uid stability (T-8).

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
- `cli/src/bootstrap/suttacentral.rs` — `get_titles()` made `pub` for FR-2b.
- `cli/src/bootstrap/tipitaka_xml.rs` — the computed coverage class replacing `CHECKED_XML_WHITELIST`, per-toc-leaf division records, and the whole-file fallback (FR-14, FR-15, FR-19, FR-20, FR-22).
- `tipitaka-xml-parser/src/…` — only if a split helper belongs beside `nikaya_structure.rs` / `encoding.rs`; do not add a second decoder (FR-4).
- `bridges/src/sutta_bridge.rs` — the bridge functions (FR-50, FR-52).
- `bridges/build.rs` — register the new QML files in `qml_files`.
- `bridges/assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` — `qmllint` stubs (T-7).
- `cpp/window_manager.cpp` / `.h` — `create_sutta_navigation_window()`, the new list, close handling (FR-23 to FR-25).
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
  - **`CHECKED_XML_WHITELIST` is a hard-coded Rust array, and it is not the
    fully-covered set.** Measured per file over `fragments.sqlite3`: **28**
    files have every Sutta fragment mapped, **22** are partially mapped (98%
    down to 6%), **2** have fragments but none mapped, and 165 have no
    fragments. The constant's 30 entries include four *partial* files
    (`s0101a.att.xml` / `s0101t.tik.xml` at 13 of 14, `s0201a.att.xml` /
    `s0201t.tik.xml` at 50 of 51) whose unmapped fragments Phase 1 `continue`s
    past while Phase 2 excludes the file (`tipitaka_xml.rs:246`) — so **four
    suttas are silently absent from the app today** — and it omits two *fully*
    mapped files, `s0305a.att.xml` (230) and `s0305m.mul.xml` (605), leaving 835
    mapped suttas unreachable. Computing the class is task 4.1 and is a
    prerequisite for success metric 6. The predicate is **full coverage**, not
    "has any `sc_code`": the latter would import 2 of `s0401m.mul.xml`'s 33
    suttas and lose 31.
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

- [ ] 4.0 CST coverage classes, division records, dual-identity rows, and the coverage report
  - [ ] 4.1 **Replace `CHECKED_XML_WHITELIST` with a computed coverage class.**
        Derive each file's class from `fragments.sqlite3` — fully mapped only if
        **every** `frag_type='Sutta'` fragment carries an `sc_code` *and* the
        fragments leave no interior gap in the file's line span (FR-14, FR-15).
        A weaker "has any `sc_code`" predicate is a **data-loss defect**:
        `s0401m.mul.xml` has 2 of 33 mapped, and promoting it would keep 2
        suttas and lose 31. Re-run the import and report the class counts.
  - [ ] 4.2 Confirm the reclassification against the measured baseline before
        trusting it: **28** files fully mapped, **22** partially, **167**
        unmapped. The four whitelist entries that are *partial* —
        `s0101a.att.xml` / `s0101t.tik.xml` (13 of 14) and `s0201a.att.xml` /
        `s0201t.tik.xml` (50 of 51) — must move to the partially-mapped class,
        which recovers the four suttas Phase 1 silently drops today. The two
        fully-mapped files missing from the whitelist, `s0305a.att.xml` (230)
        and `s0305m.mul.xml` (605), must move into fragment import, which makes
        835 already-mapped suttas reachable.
  - [ ] 4.3 Implement the split in `cli/src/bootstrap/tipitaka_xml.rs` using the
        romn `rend="chapter"` markup **plus a leading division where body text
        precedes the first chapter**, producing one record per toc leaf named to
        line up with it (`s0305m.mul11.xml/pli/cst`) (FR-20). The method is
        already chosen by measurement — record the reasoning in a comment rather
        than re-deciding it. Define "body text" explicitly: a head of only
        `rend="nikaya"` / `"book"` / `"title"` / `"centre"` paragraphs is **not**
        a division.
  - [ ] 4.4 **Drop the whole-file record wherever a finer partition covers the
        file** — for the fully-mapped class (fragments cover it) and for every
        file the split succeeded on (divisions cover it). Keeping both is the
        74 MB duplication FR-14 exists to prevent.
  - [ ] 4.5 Implement the per-file cross-check: divisions produced must equal toc
        leaves for that file; report every file where they disagree (FR-20, FR-8)
        and do not silently accept either count. Expect exactly one today —
        `abh05t.nrf.xml` (5 leaves, 6 chapters, plus a leading head).
  - [ ] 4.6 Keep the whole-file `…xml/pli/cst` import as the fallback for any file
        the split could not partition, so no text is ever lost (FR-22); flag it
        in the report rather than blocking the import.
  - [ ] 4.7 Emit division nodes as leaves labelled with the toc leaf's `text=`
        title under their toc parent (FR-19), carrying `cst_file` so the row can
        say which source file it came from.
  - [ ] 4.8 For the **fully mapped** class, emit each toc leaf as a **branch**
        whose children are that leaf's fragments (FR-18). Join on the fragment's
        group level — verified exact for `s0305m.mul.xml`, whose twelve toc leaf
        titles are byte-identical and in the same order to the distinct
        `json_extract(group_levels,'$[2].title')` values — falling back to
        `start_line` ordering. Report a leaf matching no fragment as a defect
        (FR-8) rather than guessing.
  - [ ] 4.9 For the **partially mapped** class, emit each mapped fragment as a
        **sibling row** after its division row, in source order, with a field the
        QML can use to render "in `s0201a.att.xml`" (FR-16, FR-17). Fragments are
        never nested inside the division row.
  - [ ] 4.10 Confirm FR-15 by construction: the class is computed and the
        fragment rows are produced by querying which mapped uids exist, so adding
        mappings and re-running the bootstrap changes only which class a file
        falls in — no code path enumerates files.
  - [ ] 4.11 Run the full bootstrap; record the FR-21 counts and the class counts
        as the baseline, confirm the `suttas` row count and total `content_plain`
        moved the way the classes predict (no net duplication), and confirm every
        one of the four CST roots reaches an openable leaf (success metric 2).
  - [ ] 4.12 Create `coverage_report.rs` emitting: every toc leaf not assigned to
        a record or a populated branch; every `romn/` file unreachable from any
        toc leaf; each file's coverage class with mapped/total fragment counts;
        and **counts for all of them**, so a regression is a number (FR-21).
  - [ ] 4.13 Add the `ANALYZE` call on appdata after the tree tables are
        populated (FR-12) and add the new write path to the table in
        `docs/user-data-and-sqlite-analyze.md`.

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

- [ ] 5.0 Rust bridge API for the tree, plus `qmllint` stubs
  - [ ] 5.1 Read `SuttaBridge::load_topic_index` / `get_topic_index_letters`
        (`bridges/src/sutta_bridge.rs:5710`, `:5730`) as the precedent for
        function shape, JSON serialization and signal use.
  - [ ] 5.2 Add the `nav_tree_*` functions to the `#[cxx_qt::bridge]` block and
        implement them over `simsapa_backend::navigation_tree`.
  - [ ] 5.3 Serialize nodes with every field the row rendering needs:
        `node_uid`, `title_pali`, `title_en`, `blurb`, `sutta_uid`, `cst_file`,
        `node_type`, `tree_source`, `has_children`, child count, an
        `is_openable` flag (FR-42) and the fragment's parent-file label (FR-17).
  - [ ] 5.4 Make the unavailable case explicit: when `nav_tree_is_available()` is
        false the window shows "navigation index not available", never an empty
        tree (FR-13).
  - [ ] 5.5 Add `open_sutta_navigation_window()` plus its FFI callback, following
        `open_topic_index_window()`.
  - [ ] 5.6 Add matching stubs with correct signatures and trivial return values
        to `bridges/assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` (T-7).
  - [ ] 5.7 Use `crate::queue_or_log()` for any worker that queues back to the Qt
        thread (FR-52); add none that `.unwrap()`s.
  - [ ] 5.8 `make build -B`, `cd backend && cargo test`, `make qml-lint`.

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

- [ ] 6.0 Window lifecycle: `SuttaNavigationWindow` C++ wrapper, `WindowManager` hybrid family, entry points
  - [ ] 6.1 Read `docs/window-lifecycle-and-reuse.md` §0 and §7 before writing
        any of this, and note which of §7's steps do **not** apply to a
        hide-on-close window.
  - [ ] 6.2 Create `cpp/sutta_navigation_window.{h,cpp}` modelled on
        `cpp/topic_index_window.{h,cpp}`, guarding
        `m_root = rootObjects().isEmpty() ? nullptr : …constFirst();`.
  - [ ] 6.3 Add `sutta_navigation_windows` to `WindowManager` and
        `create_sutta_navigation_window()` using `reuse_or_evict()` +
        `show_and_activate_window()` — and **no** `clear_all_tabs()` analogue,
        since preserving state is the whole point (FR-24).
  - [ ] 6.4 Make the QML `onClosing` **accept** the close and simply **not**
        route to `notify_window_closed` / `on_window_closed` — the wrapper is
        never destroyed, so there is no destruction to defer (FR-24, T-8). Do
        **not** reject the close or assign `visible = false`:
        `maybeLastWindowClosed()` runs only inside `case QEvent::Close` and only
        when the event was accepted (`qwindow.cpp:2700-2716`), so rejecting it
        is what would leave a live app with no windows. Accepting it makes Qt
        call `destroy()`, which releases the platform window but leaves
        `m_root`, the engine and all QML state intact.
  - [ ] 6.5 Read `save_session_now()` and the startup restore and **confirm by
        reading** that neither can see the new window; if either could, exclude
        it explicitly (FR-25, resolved question 6).
  - [ ] 6.6 Read `last_open_sutta_search_window()` /
        `first_open_sutta_search_window()` and confirm they remain
        sutta-window-only (FR-24).
  - [ ] 6.7 Add the menu-bar item to `SuttaSearchWindow.qml` beside
        `&Topic Index...` and the drawer-menu entry for mobile (FR-26).
  - [ ] 6.8 Route both entry points through the one `WindowManager` accessor, so
        the single-instance guarantee holds regardless of which
        `SuttaSearchWindow` asked (FR-23, FR-26).
  - [ ] 6.9 Verify the last-window case on the built app: nothing in `cpp/` sets
        `quitOnLastWindowClosed(false)`, and with 6.4's accepted close the quit
        check does run, so closing the navigation window as the only visible
        window should quit the app exactly as closing a `SuttaSearchWindow`
        does. Confirm that, and confirm reopening after a close restores the
        prior level rather than a fresh window.
  - [ ] 6.10 `make build -B`; the window opens empty and closes without crashing.

### 7.0 QML window: two row modes, breadcrumbs, drill-down, filter bar, blurb toggle

> **Specs to keep in mind.**
> **Layout:** filter input on top, breadcrumb bar below it, the level list under
> that. Entering a level resets indentation to zero, so the deepest level uses
> the full row width (FR-37).
> **The interaction rule (FR-32): a tap on a row body expands or collapses that
> node, and nothing else.** Opening a text, entering a level and expanding a
> blurb are **buttons**. A leaf row body has no tap action at all. There is no
> hover on a touch screen to disambiguate with, so an ambiguous row is a row
> that does the wrong thing.
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

- [ ] 7.0 QML window: two row modes, breadcrumbs, drill-down, filter bar, row components, blurb toggle
  - [ ] 7.1 Create `SuttaNavigationWindow.qml` with the root layout, register it
        and every new component in `bridges/build.rs`'s `qml_files` using the
        exact `"assets/qml/<Name>.qml"` form (no `..`).
  - [ ] 7.2 Implement the level model: a list populated from
        `nav_tree_children_json()`, replaced wholesale when the enter-level
        button is pressed (FR-34), with expansion state not carried across.
        Keep the query off the first-paint path (FR-50).
  - [ ] 7.3 Implement `NavigationBreadcrumbBar.qml`: full path from root to the
        current level, pressing a segment returns to that level, middle segments
        elided on a narrow screen rather than wrapping (FR-35, FR-37a).
  - [ ] 7.4 Make a tap on a branch row's body expand/collapse that node in
        place, revealing its children as **tree rows** (FR-32, FR-33). Show the
        state with the two PNG icons exactly as `ChantingTreeList.qml` does —
        the caret is an indicator, the whole row body is the tap target (FR-36).
  - [ ] 7.4a Add the **enter-level button** to every branch row, in both modes:
        it makes that node the current level, resets indentation to zero and
        appends a breadcrumb segment (FR-34).
  - [ ] 7.5 Implement the filter input: whole-tree filter via
        `nav_tree_filter_json()`, results as a **flat list** each carrying its
        breadcrumb path (FR-27); clearing it returns to the pre-filter level
        (FR-28); a result row has no body tap action and carries the same
        buttons as a tree row — **Open** on a leaf, **enter level** on a branch
        (FR-29).
  - [ ] 7.6 Apply the mobile soft-keyboard rules to the filter input:
        `MobileKeyboardHelper`, `EnterKey.type: Qt.EnterKeySearch` with a
        matching `onAccepted`, `focus: root.is_desktop`, `Qt.ImhNoAutoUppercase`
        only (FR-30).
  - [ ] 7.7 Add the blurb toggle to the top bar, shown on by default, persisted
        in `AppSettings` — the struct carries `#[serde(default)]` at the
        container level (`backend/src/app_settings.rs:186`), so a new field
        deserializes safely against existing stored settings (FR-31) — and only this setting; breadcrumbs, filter
        text, expansion and scroll are **not** persisted (FR-25).
  - [ ] 7.8 Create `NavigationNodeRow.qml` with a `is_tree_row` property
        selecting the two modes. A leaf **level row** renders Pāli title →
        English title → uid (always visible, dimmer and smaller) → blurb →
        translation hint placeholder → **Open** button, in that order; a leaf
        **tree row** renders the first three plus the Open button and nothing
        else (FR-43, FR-33).
  - [ ] 7.9 Render a branch **level row** with title, English title, uid,
        blurb, child-count indication, caret and enter-level button; a branch
        **tree row** with title, English title, uid, caret and enter-level
        button only (FR-46, FR-33).
  - [ ] 7.10 Render **no blurb area at all** when there is no blurb — no reserved
        space, no placeholder, varying row heights accepted (FR-44).
  - [ ] 7.11 Truncate the blurb to roughly two lines and add a **button**
        ("more" / "less") that expands and collapses that row's full blurb;
        expansion is per row, not a global mode. It must be a button, not a tap
        on the text: the row body's tap belongs to expand/collapse of the node
        (FR-45, FR-32).
  - [ ] 7.12 Render a fragment row with its subdued "in `s0201a.att.xml`"
        affordance so it cannot be mistaken for a duplicate of the division row
        above it (FR-17).
  - [ ] 7.13 Render a node whose target text is absent as dimmed with its Open
        button disabled (FR-42), and render the whole-window "navigation index
        not available" state when the tree is empty (FR-13).
  - [ ] 7.14 `make qml-test`; confirm no new `qmllint` warning names a file you
        touched.

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

- [ ] 8.0 Opening a text, and the installed-language translation count
  - [ ] 8.1 Implement leaf tap → open in a **new tab** of a `SuttaSearchWindow`
        following `TopicIndexWindow.open_sutta()` (FR-38).
  - [ ] 8.2 Keep the navigation window open after opening a text, on desktop and
        mobile alike (FR-39).
  - [ ] 8.3 Desktop: raise the sutta window via `show_and_activate_window()`,
        never a raw `show`/`raise`/`requestActivate` triple (FR-41).
  - [ ] 8.4 Mobile: bring the sutta window to the front, leaving the navigation
        window hidden behind it with its state intact (FR-40).
  - [ ] 8.5 Add a backend level-count query: for the `sutta_uid`s of one
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
  - [ ] 8.6 Expose it as one bridge call taking the level's uids and returning a
        uid → count map; call it once per level render, never per row.
  - [ ] 8.7 **Do not add a cache** — both query shapes are ~10 ms against the
        real DB, and a cache only creates the stale-count defect FR-48 warns
        about. If measurement later forces one, invalidate it from
        `AppData::refresh_language_caches()` (`backend/src/app_data.rs:243`).
  - [ ] 8.8 Render the count as the small indicator of FR-43 item 5.
  - [ ] 8.9 `make build -B`, `cd backend && cargo test`, `make qml-test`.

### 9.0 Verification, success metrics, and documentation

> **Specs to keep in mind.** The success metrics are measurements to *report*,
> not claims to assert. Metric 3 (unassigned toc-leaf count = 0) is the FR-21
> report's first number; metrics 1, 4, 5 and 6 each need an explicit run.
> Device checks (metric 5, the `MobileOverlayTracker` check) must be handed to
> the user — do not drive the GUI.
> **Depends on:** all of 1.0–8.0.

- [ ] 9.0 Verification, success metrics, and documentation
  - [ ] 9.1 Run the full test suite (`make test`) and record the result.
  - [ ] 9.2 Count the taps from a cold window to `sn56.11` and report the number;
        if it exceeds five, say so and propose a level-grouping change rather
        than quietly accepting it (metric 1).
  - [ ] 9.3 Report the FR-21 coverage counts and the per-class file counts; the
        unassigned toc-leaf count must be zero (attainable — the split rule
        measures 216 of 217, with `abh05t.nrf.xml` on the FR-22 fallback), and
        the unreachable-file count is informational (metric 3).
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
  - [ ] 9.8 Write `docs/sutta-navigation-tree.md`: the two spines and why they are
        not reconciled (T-2), the **four coverage classes and the rule that a
        coarser record is kept only where no finer partition covers the file**
        (§5.3, FR-14) together with the 74 MB / trigram-index reason it exists,
        the dual-identity rule for the partially mapped class, node-uid
        stability (T-3), the split method chosen and its cross-check (FR-20),
        and the KN / Añña deviations found in 2.9 and 3.7.
  - [ ] 9.9 Amend `docs/window-lifecycle-and-reuse.md`: add the third family to
        §0 with the reasoning from FR-23 to FR-25, and extend §5b's table with
        the fact that this window has no destruction to defer (T-8).
  - [ ] 9.10 Confirm `docs/user-data-and-sqlite-analyze.md` carries the new write
        path (added in 4.10) and update `PROJECT_MAP.md` with the new files.
