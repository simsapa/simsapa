# PRD — Sutta Navigation Window

**File:** `tasks/2026-09-18-174900-prd---sutta-navigation-window.md`
**Status:** Draft for implementation
**Feature name:** `sutta-navigation-window`

---

## 1. Introduction / Overview

Today the only way to reach a text in Simsapa is the search input. A user who
does not already know a sutta's name or reference has no way to *browse* the
canon — to see that the Saṁyutta Nikāya contains a Mahāvagga, which contains the
Saccasaṁyutta, which contains the Dhammacakkappavattanavagga, which contains
`sn56.11`.

This feature adds a new top-level window, **`SuttaNavigationWindow.qml`**, which
presents the whole corpus as a navigable hierarchy — the way
[tipitaka.org](https://tipitaka.org/romn/) and
[SuttaCentral's pitaka pages](https://suttacentral.net/pitaka/sutta/linked/sn)
do — and lets the user drill down to a text and open it.

It covers more than search does: SuttaCentral only carries the **Mūla** texts,
while the CST (Chaṭṭha Saṅgāyana) data also has **Aṭṭhakathā** (commentary),
**Ṭīkā** (sub-commentary) and **Añña** (other) collections. All four are
browsable here.

**Goal:** a user can find any text in the installed corpus by understanding
where it sits, without typing a reference.

---

## 2. Goals

1. Let a user reach any sutta or Pāli text in the corpus by browsing a
   hierarchy, in at most a handful of taps, with no prior knowledge of uids.
2. Make the Aṭṭhakathā, Ṭīkā and Añña collections — which are in the database
   but effectively unreachable today — first-class browsable content.
3. Show, for each node, enough context to decide whether it is the right one:
   Pāli title, English title, uid, and (where SuttaCentral has one) a blurb.
4. Work on a narrow phone screen as well as a wide desktop window, without the
   indentation of five nesting levels squeezing the text into a column.
5. Let the user open several texts in succession without losing their place in
   the hierarchy — including on mobile, where opening a text switches windows.
6. Accommodate the fact that CST↔SuttaCentral uid mapping is **partial and will
   grow over time**, without the tree's shape or the implementation changing
   each time more mappings are added.

---

## 3. User Stories

- **As a reader new to the canon**, I want to open a navigation window and
  expand "Sutta → Linked → Saṁyutta Nikāya" to see what is inside, so that I can
  discover texts I did not know to search for.
- **As a reader who half-remembers a name**, I want to type "dhammacakka" into a
  filter box and see every matching node in the tree along with the path it sits
  on, so that I can jump straight there.
- **As a student of the commentaries**, I want to browse the Aṭṭhakathā and Ṭīkā
  collections by their traditional book structure, so that I can read a
  commentary even where it has not been given a SuttaCentral-style uid.
- **As someone who knows the CST file names**, I want to see `s0305m.mul11.xml`
  as a real, openable item, so that I can navigate the way I do on tipitaka.org.
- **As a phone user**, I want to flick a branch open to see whether it holds what
  I am after and flick it shut again, enter a level when it does rather than
  scroll through deep indentation, and find the window where I left it when I
  come back from reading a sutta.
- **As a reader comparing translations**, I want to see at a glance how many
  translations exist for a sutta in my installed languages.

---

## 4. Concepts and Terminology

These terms are used throughout the requirements.

| Term | Meaning |
|---|---|
| **Node** | Any item in the tree: a pitaka, a nikāya, a book, a vagga, a saṁyutta, or a leaf text. |
| **Branch node** | A node with children. Entering it pushes a breadcrumb. |
| **Leaf node** | A node that opens a text when tapped. |
| **Mūla** | The canonical text itself (SuttaCentral's whole scope, plus CST's `toc1.xml`). |
| **Mapped uid** | A SuttaCentral-style uid the project has assigned, e.g. `sn56.11/pli/ms`, `mn11.att/pli/cst`. |
| **Division** | One toc leaf's worth of a CST file, imported as its own row, e.g. `s0305m.mul11.xml/pli/cst`. |
| **File record** | A whole CST XML file imported as one row, e.g. `s0201a.att.xml/pli/cst`. Kept only where no finer partition covers the file (FR-14). |
| **Fragment** | A mapped-uid text that is a *portion* of a larger CST file. `mn11.att/pli/cst` is a fragment of `s0201a.att.xml`. |

---

## 5. Functional Requirements

### 5.1 Tree data — sources and construction

**FR-1.** The tree must be built at **bootstrap time** (in the `cli/` bootstrap
code) and stored in the **appdata database** in new tables (see §5.2). It must
not be assembled at runtime from JSON assets or from ArangoDB.

**FR-2.** For texts SuttaCentral covers (the Mūla sutta, vinaya and abhidhamma
texts), the hierarchy must come from the **local `sc-data` checkout**, which
already holds the whole structure as files:

| Data | Path (relative to `bootstrap-assets-resources/sc-data/`) |
|---|---|
| Top-level grouping (`sutta`/`vinaya`/`abhidhamma` → `long`/`linked`/…) | `structure/tree/super-tree.json` |
| Per-collection trees | `structure/tree/sutta/*-tree.json`, `structure/tree/vinaya/`, `structure/tree/abhidhamma/` |
| Blurbs | `additional-info/blurbs.json` |
| Acronyms and collection names for uids | `misc/uid_expansion.json` |

**FR-2a. The SC spine covers the Theravāda Pāli canon only.** SuttaCentral's
structure spans the Chinese, Sanskrit, Tibetan and Gāndhārī collections too, and
Simsapa has no texts for them. Building them would fill the tree with dead
branches, so the spine is **restricted at the source**, not pruned afterwards:

| Root | Included | Excluded |
|---|---|---|
| `sutta` | `long/dn`, `middle/mn`, `linked/sn`, `numbered/an`, and all 20 members of `minor/kn` | `da`, `da-ot`, `ma`, `ma-ot`, `sa`, `sa-2`, `sa-3`, `sa-ot`, `ea`, `ea-2`, `ea-ot`, and the `dharmapadas` / `minor-lzh` / `minor-*` groupings |
| `vinaya` | the `pli-tv-vi` group only (`pli-tv-bu-vb`, `pli-tv-bi-vb`, `pli-tv-kd`, `pli-tv-pvr`, `pli-tv-bu-pm`, `pli-tv-bi-pm`) | `lzh-mg-vi`, `san-mg-vi`, `san-lo-vi` and the other schools |
| `abhidhamma` | the `pli-tv-ab` group only (`ds`, `vb`, `dt`, `pp`, `kv`, `ya`, `patthana`) | `lzh-dg-ab`, `lzh-sarv-ab`, `san-sarv-ab` |

`structure/tree/sutta/` holds 67 collection trees; this scope reaches 24 of
them. The `dharmapadas` grouping is excluded even though `dhp` is Pāli, because
`dhp` is already reachable under `minor/kn` and a second path to it would
duplicate the node.

> **The scope is fully populated — verified against the shipped
> `appdata.sqlite3`.** All 20 KN members have Pāli rows (`kp` 9, `dhp` 26,
> `ud` 80, `iti` 112, `snp` 73, `vv` 85, `pv` 51, `thag` 264, `thig` 73,
> `tha-ap` 563, `thi-ap` 40, `bv` 29, `cp` 35, `ja` 547, `mnd` 16, `cnd` 23,
> `ps` 31, `ne` 37, `pe` 9, `mil` 248). So within this scope there is no
> empty-branch problem to solve, and FR-8's unresolvable-leaf reporting is the
> only handling needed. **Should the corpus later gain a non-Theravāda
> collection, widening this table is the change** — not adding a pruning pass.

**FR-2b. Titles come from ArangoDB's `names` collection, not from
`uid_expansion.json`.** `uid_expansion.json` is a flat list of
`{uid, acronym, name}` with **one** name per uid, so it cannot supply FR-43's
Pāli-and-English pair; it is the source for the **acronym** and for a
collection's display name, nothing more. The per-text Pāli and translated
titles come from the `names` collection, which the bootstrap already reads:
`get_titles(db, lang)` in `cli/src/bootstrap/suttacentral.rs:202` returns a
uid → title map, querying `FOR x IN names FILTER x.is_root == true` for Pāli and
`FILTER x.lang == @language` otherwise. Reuse it; do not write a second reader.
For leaf suttas the imported `suttas` rows (`title_pali` / `title`) are the
fallback where `names` has no entry.

**FR-3.** For the Aṭṭhakathā, Ṭīkā and Añña collections, the hierarchy must come
from the **CST website's table-of-contents files** in
`bootstrap-assets-resources/tipitaka-org-vri-cst/tipitaka-xml/tipitaka.org/romn/`:

| File | Root it defines |
|---|---|
| `toc1.xml` | Tipiṭaka (Mūla) — used only as a cross-check / for texts SC lacks |
| `toc2.xml` | Aṭṭhakathā |
| `toc3.xml` | Ṭīkā |
| `toc4.xml` | Añña |
| `cscd/<book>.toc.xml` | The inner levels of one book, referenced by `src=` from the above |

A `<tree>` element with a `src=` attribute is a branch whose children are in the
named file; one with an `action=` attribute is a leaf pointing at a CST XML file.

**FR-4.** These files are **UTF-16 encoded**. The bootstrap must decode them
using the existing encoding helper in the CST import path (see
`tipitaka-xml-parser/src/encoding.rs` and the CLI's existing CST conversion
helpers). Do not write a second decoder.

**FR-5.** Where the existing `tipitaka-xml-parser` already defines a nikāya's
level structure (`src/nikaya_structure.rs` — `NikayaStructure::from_nikaya_name`
gives DN, MN, SN, AN, KN level lists, and `GroupType` names the levels), **reuse
those level names** for the node's displayed level label rather than inventing a
parallel vocabulary.

**FR-6.** The top level of the window shows two groups of roots:

```
Sutta / Vinaya / Abhidhamma          (from sc-data super-tree.json,
                                      Theravāda Pāli scope only — FR-2a)
Aṭṭhakathā / Ṭīkā / Añña             (from CST toc2/toc3/toc4)
```

**FR-7.** The build must be **deterministic and ordered**. Every node carries an
explicit `sort_index` reflecting its order in the source file; the UI never
re-sorts.

**FR-8.** Defects in the source data (a `src=` pointing at a missing file, a
duplicate uid, a leaf whose target text is not in the database) must be
**reported, never repaired** — logged by the bootstrap with the offending
identifier, and the node either omitted or marked unavailable. The bootstrap
must not silently invent titles or guess mappings.

### 5.2 Storage

**FR-9.** Add a Diesel migration under `backend/migrations/appdata/` creating a
node table, indexed on `parent_uid` and on `node_uid`. Suggested shape (the
implementer may refine the column set, but every listed capability must be
expressible):

| Column | Purpose |
|---|---|
| `node_uid` | Stable unique id for the node (e.g. `sn56`, `cst:s0305m.mul.toc:3`). |
| `parent_uid` | Nullable; null for a root. |
| `sort_index` | Order among siblings. |
| `node_type` | `root`, `group`, `book`, `vagga`, `samyutta`, `sutta`, `file`, … (use `GroupType` names where they apply). |
| `tree_source` | `sc` or `cst` — which spine this node came from. |
| `title_pali` | Pāli title as shown. |
| `title_en` | English title, where one exists. |
| `blurb` | SuttaCentral blurb text, nullable. |
| `sutta_uid` | The `suttas.uid` this leaf opens, nullable for branches. |
| `cst_file` | The originating CST XML file name, nullable. |
| `has_children` | Denormalized, so a row can be rendered without a child query. |

**FR-10.** The table must be queryable **one level at a time**
(`WHERE parent_uid = ?`) — the UI must never need to load the whole tree to
render a level.

**FR-11.** A separate, indexed path column or a recursive-CTE query must make the
whole-tree filter (FR-27) answerable in SQL, including reconstructing each match's
breadcrumb path for display.

**FR-12.** After the bootstrap populates these tables, run `ANALYZE` on the
appdata database, per
[docs/user-data-and-sqlite-analyze.md](../docs/user-data-and-sqlite-analyze.md),
and add the new path to that document's table.

**FR-13.** Because the tables are new and empty for existing installs, the
migration alone is sufficient — but every read must tolerate the tables being
**absent or empty** (a migration failure is non-fatal by design; see
[docs/database-migrations.md](../docs/database-migrations.md)). With no data,
the window must show an honest "navigation index not available" message, not an
empty tree that looks like a corpus with nothing in it.

### 5.3 CST records — one text, one record, at the finest granularity available

This is the subtlest part of the feature. Read it carefully.

A CST XML file can be represented in the database at three granularities. The
governing rule is that **a coarser record is kept only where the finer ones do
not cover the file**, so the same Pāli text is never stored twice:

| Granularity | Uid form | Source |
|---|---|---|
| **Fragment** | `mn11.att/pli/cst` | a `sc_code`-mapped row in `fragments.sqlite3` |
| **Division** | `s0305m.mul11.xml/pli/cst` | one toc leaf of the file (FR-20) |
| **Whole file** | `s0201a.att.xml/pli/cst` | the entire `romn/` source file |

**Why this rule and not "keep everything".** The whole-file records carry
**74 MB of the `suttas` table's 163 MB of `content_plain`** — 45% of all
indexable sutta text (measured in the shipped `appdata.sqlite3`).
`suttas_fts` is a *trigram* index over that column and the Tantivy index
directory is 841 MB, so a duplicated CST corpus costs roughly 45% on both
indexes in a database users download — and makes every ContainsMatch /
FulltextMatch hit in a CST text appear twice, once as the fragment or division
and once as the whole file. Duplication is a search defect before it is a size
problem.

**FR-14. Coverage classes decide which records a file gets.** For each of the
217 source files, classify it by how completely `fragments.sqlite3` covers it,
and import accordingly:

| Class | Predicate | Records produced |
|---|---|---|
| **Fully mapped** | every `frag_type = 'Sutta'` fragment has an `sc_code`, **and** the fragments are gapless | fragment records only — **no** whole-file record, **no** division records |
| **Partially mapped** | some but not all fragments carry an `sc_code` | division records (FR-20) **plus** the mapped fragment records |
| **Unmapped** | no fragments, or none with an `sc_code` | division records only |
| **Unsplittable** | the FR-20 split failed its cross-check | the whole-file record, as the FR-22 fallback |

The whole-file record therefore survives only in the last row. A partially
mapped file's unmapped remainder stays findable because the division records
cover the *whole* file, not only its unmapped part — which is the same
guarantee a whole-file record gave, at a finer granularity. The accepted cost
is that the mapped portion of a partially mapped file is stored twice: measured
today that is ~4 MB across 22 files, against 163 MB of sutta text.

> **Measured coverage, 2026-09-19, over `fragments.sqlite3`:**
>
> | Class | Files | Whole-file rows in the shipped DB |
> |---|---|---|
> | Fully mapped | **28** | 2 (`s0305a.att.xml`, `s0305m.mul.xml`, 0.9 MB) |
> | Partially mapped | 22 | 18 (5.6 MB) |
> | Unmapped | 167 (2 with fragments, 165 without) | 167 |
>
> Coverage among the partially mapped files ranges from 98% (`s0201a.att.xml`,
> 50 of 51) down to 6% (`s0401m.mul.xml`, 2 of 33).
>
> **Gaplessness is part of the predicate, and it was checked against the file
> text rather than only the fragment rows** — because the risk the class is
> guarding against is losing text the reader could otherwise find. Over the
> line spans in `fragments.sqlite3`, all 28 fully mapped files have their Sutta
> fragments **contiguous, with zero internal gap lines**, leaving only a 5–8
> line head (XML declaration, nikāya and book title markup) and a 3-line tail
> (closing tags). Dropping their whole-file record therefore loses nothing but
> boilerplate. The bootstrap must re-derive this per file rather than trust the
> measurement: a file whose fragments are all mapped but leave an interior hole
> is **not** fully covered, and must be treated as partially mapped.

**FR-15. The import predicate is the coverage class, computed from the data —
never a hard-coded file list.** Adding new mappings and re-running the
bootstrap must move a file between classes by itself, with no code change, no
change to the tree's shape and no change to the QML.

> **This is not true of the code as it stands, and making it true is in scope.**
> `CHECKED_XML_WHITELIST` in `cli/src/bootstrap/tipitaka_xml.rs` is a hard-coded
> array of 30 file names, and **it is not the fully-mapped set**:
>
> - Four of its entries are *partial* — `s0101a.att.xml` and `s0101t.tik.xml`
>   at 13 of 14, `s0201a.att.xml` and `s0201t.tik.xml` at 50 of 51. Phase 1
>   `continue`s past a fragment with no `sc_code`, and Phase 2 excludes
>   whitelisted files (`tipitaka_xml.rs:246`), so **those four suttas are
>   silently absent from the app today**.
> - Two *fully mapped* files are missing from it — `s0305a.att.xml` (230
>   fragments) and `s0305m.mul.xml` (605) — so 835 already-mapped suttas are
>   unreachable behind the constant.
>
> The predicate must therefore be **full coverage**, computed per file. A
> weaker "has any `sc_code` row" predicate is a data-loss defect: it would
> promote `s0401m.mul.xml` to fragment-only import, keeping 2 suttas and losing
> the other 31.

**FR-16. A partially mapped file's fragment rows are siblings of its division
rows**, under the same parent, in source order — not a secondary line on a
division's row, and not nested inside it. Both are independently openable. This
is the only class in which the two coexist.

**FR-17. A fragment row must visibly indicate which file it is a portion of**
(e.g. a subdued "in `s0201a.att.xml`" affordance or an indent marker), so it is
not mistaken for a duplicate copy of the division above it.

**FR-18. A fully mapped file's toc leaves become branches, not records.** Since
such a file produces no division records, each of its toc leaves is a **branch
node whose children are the fragments falling under it**. This is what keeps
the finest granularity available to the user without storing the text twice.

> **The join is exact and was verified.** For `s0305m.mul.xml` the twelve toc
> leaf titles (`1. Maggasaṃyuttaṃ`, `2. Bojjhaṅgasaṃyuttaṃ`, …) are
> **byte-identical, and in the same order,** to the distinct values of
> `json_extract(group_levels, '$[2].title')` over that file's Sutta fragments.
> Join on the group level, falling back to `start_line` ordering; report a leaf
> that matches no fragment as a defect (FR-8) rather than guessing.

### 5.3a Splitting the CST sources to match the toc

**FR-19. Every CST toc leaf must resolve to an openable record or a populated
branch.** There are to be no non-openable placeholder leaves in the CST spine.

> **Starting point, measured in the shipped `appdata.sqlite3`:** all 217 source
> files are *already* imported — 187 as whole-file `…xml/pli/cst` records, and
> 30 (the `CHECKED_XML_WHITELIST`) as 893 `.att` + 899 `.tik` + 1,382 `.mul`
> mapped fragments, 3,361 CST rows in total. What FR-19 adds is therefore
> **not** a bulk import but the per-toc-leaf subdivision of the whole-file
> records, and the reclassification of FR-14.

The shape of the work, measured against the checkout:

> **There are two different `romn/` directories in the checkout, and they hold
> different things.** Paths below are relative to
> `bootstrap-assets-resources/tipitaka-org-vri-cst/tipitaka-xml/`.

| Fact | Value (measured 2026-09-19) |
|---|---|
| Source XML files | 217, in **`romn/`** — the transliteration folder, which is what `TipitakaXmlImporter` is already pointed at |
| Toc files | 217 `*.toc.xml`, in **`tipitaka.org/romn/cscd/`** (219 entries in total; the other two are `tipitaka-latn.css` and `tipitaka-latn.xsl`) |
| Pairing | **exactly 1:1** — zero source files without a toc, zero tocs without a source |
| `tipitaka.org/romn/` itself | only **5** XML files: `toc1.xml` … `toc4.xml` and `tipitaka_toc.xml`. No source texts live here. |
| Split files the toc leaves name (`s0305m.mul11.xml`) | present only under `tipitaka.org/deva/cscd/` (2,917 files), **not** under either `romn/` |
| Example | `s0305m.mul.toc.xml` has 12 leaves; `deva/cscd/` holds exactly `s0305m.mul0.xml` … `s0305m.mul11.xml`; `romn/s0305m.mul.xml` carries exactly 12 `rend="chapter"` paragraphs |

So a toc leaf is **a division within** the corresponding `romn/` source file, and
the Devanagari split files are an existing authority for where the division
boundaries fall. The romn site serves these splits by transliterating at build
time — they are the same documents in a different script, not different content.

**FR-20. The bootstrap must split the CST romn sources to match the toc**, for
every file not in the fully-mapped class. For each `romn/<name>.xml` with a
`romn/cscd/<name>.toc.xml`, produce one record per toc leaf, named so that it
lines up with the leaf directly (`s0305m.mul11.xml/pli/cst`). **The division
records replace that file's whole-file record** — they partition the same text,
so keeping both is the duplication FR-14 exists to prevent.

The split rule is: **split at each `rend="chapter"` paragraph, plus a leading
division where body text precedes the first one.** The implementation must
verify per file that the number of divisions produced equals the number of toc
leaves, and report any file where they disagree (FR-8).

> **Measured across all 217 files (2026-09-19), and the residue is one file.**
>
> | Rule | Files where the division count equals the toc leaf count |
> |---|---|
> | `rend="chapter"` count alone | 169 exact; 47 at leaves = chapters **+ 1**; 1 at leaves = chapters − 1 |
> | `rend="chapter"` **plus the leading division** | **216 of 217** |
> | `deva/cscd/` split-file count | 193 exact; 24 short by one |
>
> The `+1` is explained: those 47 files open with a titled division the chapter
> markers do not open — `abh01a.att.xml` begins with `Ganthārambhakathā`, which
> is precisely its first toc leaf. The sole residue is **`abh05t.nrf.xml`**
> (5 toc leaves, 6 chapters, plus a leading head), which falls to FR-22.
>
> **"Body text" must be defined precisely**, because the count turns on it: a
> head containing only `rend="nikaya"` / `rend="book"` / `rend="title"` /
> `rend="centre"` paragraphs is *not* a division. The per-file cross-check this
> requirement mandates is what catches a wrong definition.
>
> **Use the romn markup, not the `deva/cscd/` counts.** It keeps the whole
> feature inside the directory the importer already reads, avoids depending on
> the Devanagari tree and its transliteration assumption, and reaches 216 of
> 217 against the deva method's 193.

**FR-21. The bootstrap must emit a CST coverage report.** Add a verification pass
that runs during the CST import and reports:

1. Every toc leaf that could **not** be assigned to an imported record or branch.
2. Every XML file in `romn/` that is **not** reachable from any toc leaf.
3. Each file's coverage class (FR-14) and, for a partially mapped file, its
   mapped/total fragment counts.
4. Counts for all of the above, so a regression is visible as a number rather
   than requiring the log to be read line by line.

**FR-22. A file the bootstrap could not split is still imported whole**, under
the existing `…xml/pli/cst` uid pattern, so its text is never lost. It appears
in the tree under its nearest resolvable parent, flagged by the report of FR-21
as unsplit. The report is the signal to fix the mapping; the import is never
blocked on it. On today's data this fallback is expected to fire for exactly one
file, `abh05t.nrf.xml`.

### 5.4 Window, lifecycle and entry points

**FR-23. There is at most one navigation window, ever.** It is a
**single-instance** window in the user-facing sense — no menu action, no
keyboard shortcut and no code path may produce a second one. Requesting it when
one already exists always revives the existing instance.

**FR-24. Closing it does not destroy it.** This is the one way it departs from
the other secondary windows. The close is **accepted** — the QML `onClosing`
must not reject it, and must not assign `visible = false` — but it is **not**
routed to `notify_window_closed`, so the instance stays in the `WindowManager`
list with its `QQmlApplicationEngine` alive and reopening restores the
breadcrumb path, expansion state, filter text and scroll position **without any
save/restore code**: the object was never torn down.

> **Accepting the close is load-bearing, and rejecting it is the bug.** Qt calls
> `QGuiApplicationPrivate::maybeLastWindowClosed()` — the quit-on-last-window
> check — **only** inside `case QEvent::Close` and **only if the event was
> accepted** (`qwindow.cpp:2700-2716`). Reject the close to keep the window
> alive and that check never runs, so closing this window while it is the only
> visible one leaves a live app with no windows and no way back. Accepting it
> makes Qt call `destroy()`, which releases the *platform* window but not the
> `QQuickWindow`: `m_root` stays non-null, all QML state survives, the
> `m_root != nullptr` reuse predicate still finds it, and `setVisible(true)`
> recreates the surface on the next open. See the settled last-window item
> at the end of §10.

> **This is a hybrid of the two families in
> [docs/window-lifecycle-and-reuse.md](../docs/window-lifecycle-and-reuse.md),
> and that document's §0 dichotomy no longer covers every window once this
> ships.** Read §0 and §7 before implementing, then update the document with a
> third row. Concretely:
>
> - The reuse predicate is **`m_root != nullptr`**, as for the single-instance
>   family — *not* `visible`. `SuttaSearchWindow` uses `visible` because there
>   are many of them and a hidden one is a closed one the user should not have
>   re-raised. Here a hidden window is *the* window, waiting to be shown again,
>   so a `visible` predicate would wrongly construct a second instance every
>   time the user reopens it after closing.
> - The **null-`m_root` eviction** rule still applies: a failed engine load must
>   be evicted rather than skipped, or one dead wrapper leaks per open.
> - Unlike `SuttaSearchWindow`'s revive path, the navigation window's revive
>   must **not** clear its state — no `clear_all_tabs()` analogue. Preserving
>   state is the entire reason it is not destroyed.
> - It must **not** appear in the `window_id`-less dispatch fallbacks
>   (`last_open_sutta_search_window()` / `first_open_sutta_search_window()`),
>   which are about the sutta-window pool only.

**FR-25. It takes no part in session save or restore.** The navigation window is
deliberately excluded from `gui.cpp`'s `aboutToQuit` session save and from
startup session restore. State survives close/reopen **within a session** and is
gone on app restart, which is the intended behaviour — do not add persistence to
`AppSettings` for it. (The blurb toggle of FR-31 is a *setting* and does persist;
breadcrumbs, filter text, expansion and scroll do not.)

**FR-26.** Entry points:

- A menu item in `SuttaSearchWindow`'s menu bar, alongside the existing
  `&Topic Index...` entry.
- A toolbar / drawer-menu entry, so it is reachable on mobile.
- Both entry points route through one `WindowManager` accessor, so the
  single-instance guarantee of FR-23 holds regardless of which was used, and
  regardless of which `SuttaSearchWindow` the request came from.

### 5.5 Top bar — filter and controls

**FR-27.** A **filter input** at the top of the window filters across the
**whole tree**, not just the current level. Matches are shown as a **flat list**,
each row carrying its full breadcrumb path so the user can see where it sits.
Matching applies to the Pāli title, the English title and the uid.

**FR-28.** Clearing the filter returns to the tree at the level the user was on
before filtering.

**FR-29.** Filter results are a flat list with nothing to expand, so a result
row has no tap action on its body either (FR-32). It carries the same buttons as
a tree row: **Open** on a leaf (FR-38), **enter level** on a branch, which
navigates to that level and sets the breadcrumbs to its path.

**FR-30.** The filter input must follow the mobile soft-keyboard rules in
[docs/android-soft-keyboard.md](../docs/android-soft-keyboard.md): use
`MobileKeyboardHelper.qml`, `EnterKey.type: Qt.EnterKeySearch` with a matching
`onAccepted`, `focus: root.is_desktop`, and `Qt.ImhNoAutoUppercase` only.

**FR-31.** The top bar also carries a **blurb toggle**. Blurbs are shown **by
default**; the toggle hides them for a compact view. The setting persists across
sessions in `AppSettings`. It governs **level rows only** — tree rows never
show a blurb (FR-33).

### 5.6 The two row modes, breadcrumbs, and drill-down navigation

**FR-32. Tapping a row expands or collapses it; every other action is a
button.** A tap anywhere on a branch row's body toggles that node open or
closed. A tap never opens a text, never enters a level and never expands a
blurb — those are explicit buttons (FR-34, FR-38, FR-45). This is the one
interaction rule the row layout must not blur: a touch screen gives no hover to
disambiguate with, so an ambiguous row is a row that does the wrong thing.

**FR-33. Rows render in one of two modes.** The distinction is what a row *is*
in the list, not a user setting:

| Mode | Which rows | Content |
|---|---|---|
| **Level row** | a direct child of the current breadcrumb level | the full rendering of FR-43 / FR-46, blurb included when the FR-31 toggle is on |
| **Tree row** | anything revealed by expanding a row, at any depth below it | compact: Pāli title, English title where present, uid, the enter-level button — **no blurb**, whatever the toggle says |

Tree rows exist so a user can flick a branch open, see whether the sub-items are
the ones they are looking for, and flick it shut again. Blurbs would defeat
that, so they are omitted for space efficiency even when the toggle is on —
the toggle governs level rows only.

**FR-34. A branch row carries an "enter level" button.** Pressing it makes that
node the current level: the list is replaced by its children rendered as level
rows, indentation resets to zero, and a breadcrumb segment is appended.
Expansion state from the tree view is not carried into the new level.

**FR-35. A breadcrumb bar** below the filter input shows the path from a root to
the current level, e.g.
`Sutta › Linked › Saṁyutta Nikāya › Mahāvagga › Saccasaṁyutta`. Pressing an
earlier segment returns to that level.

**FR-36. Expand/collapse carets must be PNG icons, never Unicode characters** —
reuse `icons/32x32/fe--drop-down.png` and `icons/32x32/fe--drop-right.png`
exactly as `ChantingTreeList.qml` does (see its lines 243 and 338). The caret is
an indicator of the row's state; the tap target is the whole row (FR-32), not
the caret alone.

**FR-37. Because entering a level resets indentation to zero, the deepest level
needs no horizontal nesting at all** — the full row width is available for the
title and blurb even on a phone. Tree rows below an expanded level row do
indent, which is exactly why they are compact: the indentation is temporary and
the user's next action is either to enter the level or to collapse it again.

**FR-37a. The breadcrumb bar must remain usable on a narrow screen:** elide or
collapse middle segments (e.g. `Sutta › … › Saccasaṁyutta`) rather than
overflowing or wrapping to many lines.

### 5.7 Opening a text

**FR-38.** Tapping a leaf node opens that text in a **new tab** of a
`SuttaSearchWindow`, following the dispatch pattern of
`TopicIndexWindow.qml`'s `open_sutta()` (which builds `result_data` and routes
it to a sutta search window).

**FR-39.** The navigation window **stays open** after a text is opened, on both
desktop and mobile.

**FR-40.** On **mobile**, opening a text must bring the sutta search window to
the front (the navigation window is hidden behind it, keeping its state). The
user returns to the navigation window through the menu, and finds it exactly as
they left it — same level, same breadcrumbs, same filter, same scroll position —
so they can continue finding more texts.

**FR-41.** On **desktop**, the navigation window stays visible; the sutta search
window is raised via `show_and_activate_window()`, never a raw
`show`/`raise`/`requestActivate` triple.

**FR-42.** A node whose target text is not present in the database must be
rendered as **non-openable** (dimmed, no tap action) rather than opening an
empty or error tab.

### 5.8 Row rendering

**FR-43.** A leaf **level row** displays, in this order:

1. **Pāli title** (primary line).
2. **English title**, where one exists (secondary line).
3. **uid** — always visible, in a dimmer and smaller style. This is
   `sn56.11` for mapped texts and `s0305m.mul11.xml` for CST division records.
4. **Blurb**, when the blurb toggle is on.
5. **Translation availability hint** — a small indicator of how many
   translations exist for that text in the installed languages.
6. An **Open** button (FR-38). A leaf has nothing to expand, so its row body
   has no tap action at all — the button is the only way to open it.

A leaf **tree row** shows items 1–3 and the Open button, and nothing else
(FR-33).

**FR-44. A node with no blurb simply has no blurb area.** The row is shorter; no
space is reserved and no placeholder is shown. Blurb coverage is partial, and a
list of rows with varying heights is the accepted consequence.

**FR-45.** The blurb is **truncated to roughly two lines**, with a **button**
(e.g. "more" / "less") that expands that row in place to the full blurb and
collapses it again. Expansion is **per row**, not a global mode. It is a button
and not a tap on the text because the row body's tap is expand/collapse of the
node itself (FR-32); only the button may be hit to read more.

**FR-46.** A branch **level row** displays its Pāli title, English title where
present, its uid, its blurb when the toggle is on, a child-count indication, an
expand/collapse caret reflecting its state (FR-36), and the **enter-level
button** of FR-34. A branch **tree row** shows the title, English title, uid,
caret and enter-level button only.

**FR-47. The translation count means "translations installed right now."** It
counts translations of that text in the languages the user currently has
installed — not translations that exist in the corpus. It therefore **changes
when a language is downloaded or removed**, so it cannot be computed once at
bootstrap and stored.

**FR-48. Build the count efficiently.** A per-row query against the `suttas`
table is unacceptable for a level with hundreds of rows: compute counts for the
whole displayed level in one query, keyed on the installed-language set. If a
cache is introduced, it must be invalidated by the same hooks that refresh the
existing `AppSettings` language caches (see
[docs/startup-sequence-and-caches.md](../docs/startup-sequence-and-caches.md)) —
a stale count after a language download is a defect.

**FR-49.** New QML components should be written for these rows — modelled on the
chanting-practice list items but with the extended layout above. Follow the
project's QML conventions: files under `bridges/assets/qml/`, registered in
`bridges/build.rs`'s `qml_files` with the exact `"assets/qml/<Name>.qml"` form,
`Logger` for logging (never `console`), snake_case ids and properties.

### 5.9 Performance and long operations

**FR-50.** Entering a level must feel instantaneous. Levels are queried lazily
(FR-10); no level query may run on the QML thread in a way that blocks the
first paint.

**FR-51.** The bootstrap tree build is a long operation in the CLI, not in the
app, so the keep-screen-on rule does not apply. If a future in-app rebuild is
added, it must bracket the work with
`AssetManager.set_keep_screen_on("<holder>", …)` under a distinct holder name.

**FR-52.** Any bridge worker that queues back to the Qt thread must use
`crate::queue_or_log()` — never `.unwrap()` or `let _ =` — per the standing rule.

---

## 6. Non-Goals (Out of Scope)

1. **No editing of the tree.** The hierarchy is bootstrap data; there is no
   in-app update/reset path like the Topic Index's. (A future PRD may add one.)
2. **No new CST↔SC uid mappings** are produced by this feature. It displays
   whatever mapping exists and is designed so that more can be added later
   without code changes (FR-15).
3. **No translation of blurbs.** SuttaCentral's English blurbs are shown as-is;
   `blurbs.json` has other languages but selecting among them is out of scope.
4. **No parallels / relationship display.** SuttaCentral's parallels view is not
   reproduced.
5. **No bookmarking or history within the navigation window** beyond the state
   that survives hide/show.
6. **No replacement of the search bar.** This is an additional route, not a
   change to search.
7. **No ArangoDB dependency in the app.** The shipped app never queries
   ArangoDB — the tree is read from the appdata database (FR-1). The
   **bootstrap** may and does: `arangors` is already in `cli/Cargo.toml`, and
   `connect_to_arangodb()` / `get_titles()`
   (`cli/src/bootstrap/suttacentral.rs:94`, `:202`) are existing bootstrap code
   that FR-2b reuses for titles. The structural spine still comes from the
   local `sc-data` files (FR-2), which do not require a running ArangoDB.
8. **No desktop-only nested mega-tree.** Desktop and mobile use the same
   breadcrumb drill-down model; the desktop does not get a separate always-fully-
   nested rendering.

---

## 7. Design Considerations

**Reference renderings** (screenshots supplied with the request):

- `tipitaka.org/romn/` left panel — the four roots and the Saccasaṁyutta path.
- `suttacentral.net/pitaka/sutta/linked/sn` — collection level with blurbs.
- `suttacentral.net/pitaka/sutta/linked/sn/sn-mahavaggasamyutta/sn56` — saṁyutta
  level.
- `suttacentral.net/sn56-dhammacakkappavattanavagga` — the vagga's sutta list.
- `~/Screenshots/2026-09-18_13-56.png` / `2026-09-18_13-57.png` — truncated vs
  expanded blurb, which is the model for FR-45.

**Existing components to model on:**

| Need | Precedent |
|---|---|
| Expand/collapse tree with PNG carets, persistent expansion keyed by uid | `bridges/assets/qml/ChantingTreeList.qml` |
| Window layout, opening a sutta from a secondary window, `extra_top_margin` | `bridges/assets/qml/TopicIndexWindow.qml` |
| Pooled window revive semantics | `SuttaSearchWindow` + `WindowManager::create_sutta_search_window()` |

**Layout rules that apply and are easy to get wrong:**

- Any `Dialog` in this window states its own width clamped to the overlay
  (`parent: Overlay.overlay`, `width: Math.min(parent.width - 40, <max>)`), with
  `wrapMode: Text.WordWrap` inside.
- A titled dialog with wrapping text uses `header: DialogHeader { … }`.
- Never assign `topPadding`/`padding` on the `ApplicationWindow` root; anchor the
  root content item to its parent rather than sizing it from `root.width`/
  `root.height` (see
  [docs/android-edge-to-edge-and-safe-areas.md](../docs/android-edge-to-edge-and-safe-areas.md)).
- On mobile, anything drawn over the reader interacts with the webview
  visibility layers. **This was verified, and the premise it was written on is
  wrong:** `MobileOverlayTracker` is instantiated *inside* `SuttaSearchWindow`
  (`SuttaSearchWindow.qml:259`) and sees two things only — popups in **that
  window's** `Overlay.overlay`, and **declared in-tree child windows**. A
  secondary window built by its own `QQmlApplicationEngine`, which is what this
  window is (following `TopicIndexWindow`), is neither, so it will **not** be
  picked up — and neither is any existing secondary window today. This is
  therefore pre-existing, shared behaviour rather than something this feature
  must solve; it needs a **device check** (does the sutta window's native webview
  bleed through the navigation window on Android?), not tracker code. Do not add
  the new window to the walk — see that document's "runtime-created in-tree child
  windows" limitation for why the walk is the wrong instrument
  ([docs/mobile-webview-visibility-management.md](../docs/mobile-webview-visibility-management.md)).

---

## 8. Technical Considerations

**T-1. Bootstrap order.** The tree build must run *after* the sutta import, so
that `sutta_uid` references can be validated against real rows and unresolvable
leaves reported (FR-8).

**T-2. Two spines, one table, and no attempt to reconcile them.** The SC-derived
and CST-derived subtrees land in the same table, distinguished by `tree_source`.
The two builders are separate code paths joined only at the root level (FR-6).
Keep them separate — the temptation to unify them will produce a builder that
serves neither structure well.

**Each spine uses its own source's grouping, and disagreements are not
resolved.** Simsapa's sutta data is heavily based on the SuttaCentral import, so
the SC spine follows SuttaCentral's grouping wherever SuttaCentral has one, and
the CST spine follows the CST toc's grouping. Where the two disagree — the same
text sitting at a different depth, or under a differently-named level, in the
Mūla root than in the Aṭṭhakathā root — **that is the correct outcome, not a
defect**. Do not normalize one to the other, and do not report the divergence as
an error.

Consequently `NikayaStructure`'s level names (FR-5) label the **CST** spine's
levels, and SuttaCentral's own grouping names label the **SC** spine's. If
`NikayaStructure` gives AN a `Pannasaka` level and `an-tree.json` groups
differently, both are right, each in its own spine.

**T-3. Node uid stability.** `node_uid` must be stable across bootstraps, because
the UI persists expansion state and the breadcrumb path by node uid across
hide/show and across app updates. Derive it from source identifiers (SC uid, CST
file name + toc index), never from a row id or an enumeration order that could
shift.

**T-4. The Vinaya idiosyncrasy.** The layering does not line up one-to-one. The
Pācittiya 45 rule is `pli-tv-bu-vb-pc45/pli/ms`; its mūla is in
`vin02m1.mul.xml`, its aṭṭhakathā in `vin02a1.att.xml`, its ṭīkā in
`vin11t.nrf.xml` — three different files under three different roots, with
different internal structures. The design **does not attempt to unify these into
one row**: each root is browsed on its own spine (FR-6), and the dual-identity
rule (§5.3) is what makes the commentary reachable. Do not build a cross-root
"related texts" join in this PRD's scope.

**T-5. Collections differ in depth.** DN is Nikāya → Book → Sutta; MN adds a
Vagga; SN adds Saṁyutta and Vagga; AN uses Paṇṇāsaka. The tree must be
**depth-agnostic** — driven entirely by `parent_uid`, never by a hard-coded
number of levels. `NikayaStructure` supplies *labels*, not a fixed shape.

**T-6. Khuddaka and Añña are the messiest.** KN's members vary in structure and
the Añña collection is a flat-ish grab bag. These must be verified against the
real toc files during implementation rather than assumed; where a structure
cannot be derived, a flat list under the root is an acceptable outcome, and the
deviation should be noted.

**T-7. `qmllint` stubs.** Any new bridge functions need matching stubs in
`bridges/assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` (or a new bridge
component declared in `qmldir`, with the Rust file added to
`CxxQtBuilder::files([…])` in `bridges/build.rs`).

**T-7b. `sc-data` is maintained by hand.** The checkout under
`bootstrap-assets-resources/sc-data/` is updated manually when SuttaCentral data
is refreshed. The bootstrap reads whatever is there; it must not fetch, update or
validate the checkout's freshness, and no automated refresh is in scope.

**T-8. Documentation.** This feature adds a doc
(`docs/sutta-navigation-tree.md`) covering the two spines, the dual-identity
rule, and the node-uid stability requirement; and it **amends**
`docs/window-lifecycle-and-reuse.md` (§0 presents an exhaustive two-family
split — pooled-and-hidden vs single-instance-and-destroyed — and this window is
neither: one instance, hidden on close, `m_root` predicate, no session
participation; add it as a third case with the reasoning from FR-23 to FR-25, and
extend §5b's deferred-destruction table with the fact that this window has no
destruction to defer), `docs/user-data-and-sqlite-analyze.md` (new write path),
and `PROJECT_MAP.md`.

---

## 9. Success Metrics

1. A user can reach `sn56.11` from a cold window in **five interactions or
   fewer**, counting a row tap, an enter-level button and an Open button alike
   (Sutta → Linked → SN → Mahāvagga → Saccasaṁyutta → Dhammacakkappavattanavagga
   → the sutta; measured and reported — if it is more, the level grouping should
   be reconsidered). Expanding in place and entering a level are both legitimate
   routes; report which one the count assumes.
2. Every one of the four CST roots (Mūla, Aṭṭhakathā, Ṭīkā, Añña) is browsable
   to an openable leaf.
3. **Every CST toc leaf opens a real text, or is a branch over real texts — the
   unassigned count is zero.** FR-21's bootstrap coverage report is the
   measurement: it names every toc leaf that could not be assigned and every
   `romn/` file not reachable from any leaf, and prints counts for both. Zero on
   the first count is the target and is attainable — the FR-20 split rule was
   measured at 216 of 217 files, with `abh05t.nrf.xml` falling to the FR-22
   whole-file fallback. A non-zero second count is informational.
4. Entering a level renders in under 100 ms on desktop for the largest level in
   the corpus.
5. On mobile, opening a text and returning via the menu restores the exact prior
   level, filter text and scroll position — verified by hand on a device.
6. Adding a batch of new CST→SC uid mappings and re-running the bootstrap changes
   the tree contents with **no code change** (FR-15), verified by doing it once
   with a small batch.

---

## 10. Resolved Decisions and Remaining Questions

All seven questions raised while drafting have been answered. They are kept here
because each one is a decision the implementation has to honour, and the
reasoning is not recoverable from the requirement text alone.

1. **Grouping conflicts between the two spines — resolved: each spine keeps its
   own.** Simsapa's sutta data is heavily based on the SuttaCentral import, so
   the SC spine follows SuttaCentral's grouping wherever it has one, and the CST
   spine follows the CST toc's. The same text appearing at a different depth in
   the Mūla root than in the Aṭṭhakathā root is correct, not a defect. See T-2.
2. **Translation-count semantics — resolved: "installed right now."** It reflects
   the user's currently installed languages and changes with language downloads,
   so it cannot be baked in at bootstrap. See FR-47 and FR-48.
3. **Missing blurbs — resolved: render nothing.** No reserved space, no
   placeholder; the row is simply shorter and the list has varying row heights.
   See FR-44.
4. **CST split-file scope — resolved: do it in full, and verify it.** Every toc
   leaf resolves to an openable record or a populated branch (FR-18, FR-19,
   FR-20); the bootstrap emits a coverage report naming unassigned toc leaves,
   unreachable files and each file's coverage class (FR-21); and a file the
   bootstrap cannot split is still imported whole so no text is lost (FR-22).
   This is smaller than it first appeared — the toc already names the files and
   the `romn/` ↔ `romn/cscd/` pairing is 1:1.
5. **`sc-data` freshness — resolved: it is maintained by hand.** There is no
   automated refresh and none is to be added. The bootstrap consumes the
   checkout as it finds it.
6. **Hide-on-close vs the existing window families — resolved by FR-23 to FR-25
   (not FR-20 to FR-22, which are the CST split), but two existing code paths
   must be checked by reading them.** `gui.cpp`'s
   `aboutToQuit` session save must **skip** the navigation window (its array
   length is the restored *sutta* window count, so including it would corrupt
   the restore), and the `window_id`-less dispatch fallbacks must continue to
   consider only sutta search windows. Do not assume the existing filters
   already exclude it.
7. **Whether a whole-file record is kept alongside the finer records —
   resolved: only where the finer ones do not cover the file.** A coarser
   record is a duplicate wherever a finer partition already covers the same
   text, and duplication is a search defect before it is a size problem: the
   whole-file records are 74 MB of the `suttas` table's 163 MB of
   `content_plain`, indexed by a trigram FTS5 table and by Tantivy. Hence the
   four coverage classes of FR-14 — fragments only for a fully mapped file,
   divisions plus fragments for a partially mapped one (so its unmapped
   remainder stays findable), divisions only for an unmapped one, and the
   whole-file record only where the split itself failed. The accepted cost is
   the mapped portion of a partially mapped file being stored twice: ~4 MB
   across 22 files today.

**Genuinely open — to be settled during implementation, not before:**

- **Khuddaka and Añña structure** (T-6). These are the least regular parts of
  both spines and must be checked against the real toc and tree files rather
  than assumed. Where a sensible hierarchy cannot be derived, a flat list under
  the root is an acceptable outcome — but say so in the implementation notes
  rather than leaving it to be discovered later.
- ~~**Which method derives the CST split boundaries** (FR-20)~~ — **settled by
  measurement, see the table under FR-20.** Use the romn XML's own
  `rend="chapter"` markup plus a leading division where text precedes the first
  chapter; keep the mandated per-file cross-check against the toc leaf count.

- ~~**What happens when the navigation window is the only visible window and
  the user closes it** (FR-24)~~ — **settled by reading Qt's source.**
  `qwindow.cpp:2700-2716` calls `QGuiApplicationPrivate::maybeLastWindowClosed()`
  **only** inside `case QEvent::Close`, and **only if `ev->isAccepted()`**. So a
  QML `onClosing` that rejects the close and sets `visible = false` never runs
  the quit check at all, and closing the navigation window while it is the only
  visible window leaves a live app with no windows and no way back — the exact
  hazard this question was asking about.
  **Therefore: let the close be accepted, and simply do not notify the C++
  wrapper.** Qt then calls `destroy()`, which releases the *platform* window but
  not the `QQuickWindow`: `m_root` stays non-null, every piece of QML state
  (breadcrumbs, filter text, expansion, scroll position) survives, the
  `m_root != nullptr` reuse predicate still finds it, `setVisible(true)`
  recreates the surface — **and** `maybeLastWindowClosed()` fires, so the
  last-window case behaves exactly like every other window. FR-24 needs no
  `visible = false` assignment; "hidden, not destroyed" falls out of not routing
  the close to `notify_window_closed`.

---

## 11. Not to be done

Per the workflow: **this PRD is not to be implemented as part of producing it.**
Generate the task list separately.
