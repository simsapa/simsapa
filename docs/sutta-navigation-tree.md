# Sutta navigation tree

The browsable hierarchy behind the Sutta Navigation window, built at bootstrap
time into the `navigation_nodes` table of `appdata.sqlite3`. The app never
assembles it at runtime and never queries ArangoDB for it.

> Work in progress: the window itself and its bridge API are not built yet.
> What follows describes the bootstrap side, which is.

## 1. Two spines, never reconciled

| `tree_source` | Roots | Source |
|---|---|---|
| `sc` | Sutta / Vinaya / Abhidhamma | the local `sc-data` checkout |
| `cst` | Aṭṭhakathā / Ṭīkā / Añña | the tipitaka.org toc files |

Each spine follows its own source's grouping. The same text sitting at a
different depth in the Mūla root than in the Aṭṭhakathā root **is the correct
outcome, not a defect** — do not normalize one toward the other, and do not
report the divergence as an error. The two builders are separate code paths
joined only at the root level; the temptation to unify them produces a builder
that serves neither structure well.

`toc1.xml` (Mūla) is read for its `src=` targets only and is **not** built as a
fourth CST root — those texts are browsed on the SC spine, and a fourth root
would be a second path to the same material.

### 1.1 Why the Mūla division records are not reachable from the tree

Splitting runs over **every** source file, Mūla included, so `romn/` Mūla files
carry division records (`s0305m.mul11.xml/pli/cst`) that no `navigation_nodes`
row points at — 661 of them today. That is deliberate and is not a gap to close
by building a Mūla root.

Those records exist so that **no text is missing from the app** while the parser
catches up. `tipitaka-xml-parser` has mapped 30 of the 217 source files to
verified sutta-fragment uids (§3.1); until a file is on that list its suttas have
no mapped uid, and the division records are what keep its text present and
findable by search. As more files are verified, their suttas become mapped
records reachable from the SC spine, and the corresponding divisions stop being
the only route to them.

So a Mūla text is reachable one way or another at every stage: through the SC
spine once its file is verified, and through search on its division record
before that. A Mūla root in the tree would add a second browsing path to
material the SC spine already covers, which §1's rule exists to prevent.

## 2. Node uids are stable across bootstraps

The UI persists expansion state and the breadcrumb path by `node_uid`, so it is
derived from source identifiers only — never from a row id, never from an
enumeration counter.

| Node | `node_uid` |
|---|---|
| SC grouping level | qualified by its ancestors: `sutta`, `sutta/linked` |
| SC collection or text | its bare SuttaCentral uid: `sn`, `sn56`, `sn56.11` |
| CST root | fixed: `cst:atthakatha`, `cst:tika`, `cst:anna` |
| CST grouping node | parent uid + a slug of its title |
| CST `src=` branch | its toc file's stem: `cst:s0101a.att` |
| CST division | its division file name: `cst:s0305m.mul0.xml` |
| CST fragment row | `cst:frag:` + the sutta uid: `cst:frag:mn11.att/pli/cst` |

## 2a. Three columns the window depends on

### Root ordering across the two spines

The spines are built and written **independently** — either can be rebuilt on
its own — and each numbers its own roots from zero. Written as-is the root level
interleaves them (`sutta(0) atthakatha(0) vinaya(1) tika(1) …`), with the tie
broken only by whichever spine was inserted last. `root_sort_base()` in
`cli/src/bootstrap/navigation_tree/mod.rs` offsets the CST roots to 100, so the
top level is the two whole groups in order:

```
Suttapiṭaka / Vinayapiṭaka / Abhidhammapiṭaka     sort_index 0, 1, 2   (sc)
Aṭṭhakathā / Ṭīkā / Añña                          sort_index 100–102  (cst)
```

Nothing below the root level needs the offset: every other node's siblings all
come from one spine.

### `search_text` — the folded column the filter matches

The whole-tree filter **cannot** match the title columns. SQLite's `LIKE` folds
case for ASCII only and does no accent folding at all, so before this column
existed `samyutta` found 5 nodes where `saṁyutta` found 132, `saccasamyutta`
found none of the 3 `Saccasaṁyutta` nodes, and `MAHAVAGGA` found nothing.

`navigation_tree::build_search_text` folds the Pāli title, the English title,
the `node_uid` and the `sutta_uid` through `normalize_for_search`
(`consistent_niggahita` → `pali_to_ascii` → lowercase) and joins them with a
space. **`filter_nodes` folds the query with the same function**; a one-sided
fold is a silent miss, not an error, which is why there is one function and not
two. All four fields are in the column because the filter matches **branch nodes
as well as leaves** — it is how a reader who already knows where they are going
skips the drill-down, so a level name is as legitimate a thing to type as a
sutta title.

Titles are stored folded too, by `fold_title` in the single `flatten_node` pass
that every row of both spines passes through. The sources disagree — the CST toc
writes `Saccasaṃyuttaṃ`, SuttaCentral writes `Saccasaṁyutta` — and folding at the
one chokepoint is what stops a displayed title and its `search_text` drifting
apart, and stops two spellings of one nikāya name reading as two places. Blurbs
are left alone: they are prose, shown as the source wrote them.

### `sort_path` — what orders the filter's results

`path` cannot order them. It is built from `node_uid`s, so sorting by it is
alphabetical: the filter returned the **CST** roots before the SC ones
(`|cst:...` < `|sutta|`) and Añña before Aṭṭhakathā, contradicting the level
order the same user had just been browsing.

`sort_path` is the same ancestor walk in `sort_index`es, zero-padded to a fixed
width and pipe-joined (`|000000|000002|000005|`). Fixed width is what makes a
plain string compare equal a numeric one — without it `9` sorts after `10` — and
it is what makes a parent's key a **prefix** of its children's, so a parent
always precedes its own subtree and a subtree always precedes its parent's next
sibling. Ordering the filter by it also makes the `LIMIT` meaningful: the first
N in tree order, rather than the first N alphabetically.

> **Standing rule: these columns are materialized, so changing how one is
> derived means rebuilding the tree.** `search_text`, `sort_path` and `path` are
> all written by the CLI. A stale one fails **silently** — edit
> `normalize_for_search` without rebuilding and the query is folded one way
> while the column is folded another, so matches simply stop appearing with
> nothing logged anywhere. Re-run `simsapa_cli build-navigation-tree` after
> touching any of them.

### Which edition an SC leaf opens

`edition_rank()` in `sc_spine.rs`: `pli/ms` first, `pli/cst` second, any other
Pāli source third, a non-Pāli row last. The Mahāsaṅgīti is the edition the
SuttaCentral structure was built around, and the CST text of the same sutta is
browsed on the CST spine under its own toc.

Ranking is load-bearing rather than cosmetic. Taking the first Pāli row in uid
order — which is what an unranked scan does, because `dn1/pli/cst` sorts before
`dn1/pli/ms` — handed 1,379 leaves the CST edition and 5,906 the Mahāsaṅgīti, so
browsing DN and browsing SN returned different editions for no stated reason.
With the rank applied it is 7,283 `ms`, 2 `cst` (ranged uids with no `ms` row)
and 1 English (a node with no Pāli text at all, opened rather than dimmed).

The CST mūla fragments those 1,379 leaves used to open are now reached the same
way every other alternative edition is: **the reader's own translations panel**.
`get_translations_data_json_for_sutta_uid` lists every `<ref>/%` row beside the
opened one, so `dn1/pli/cst` is offered there for an opened `dn1/pli/ms` — but
**only when the user has enabled CST mūla**, which is what `include_cst_mula`
gates (the panel filters those rows out otherwise). Search reaches them
regardless.

This is the intended division of labour: the tree gives one route to a text, and
the reader gives the editions of it. Do not add a second `sutta_uid` to a node,
or a per-edition branch, to reproduce in the tree what the reader already does.

## 3. CST records: two independent questions

Which records a CST source file gets is decided by two questions that must not
be conflated. `cli/src/bootstrap/cst_coverage.rs` holds both.

### 3.1 Is the parse trustworthy? — `VERIFIED_CST_FILES`

A hand-maintained list of 30 source file names. **Only a listed file
contributes fragment records.** Everything else is imported as divisions of the
source XML, which do not depend on the parser's sutta splitting at all.

`tipitaka-xml-parser` is a work in progress. It produces **wrong splits and
wrong uid assignments** on files whose structure it does not yet handle, so a
file's fragments are usable only once someone has run the parser's validations
over it and reviewed the result. The nine checks live in
`tipitaka-xml-parser/src/web/validation.rs` (`missing_sc_code`,
`unattached_text`, `missing_sc_sutta`, `sutta_review_status`,
`xml_reconstruction`, `sc_code_sequence`, `code_uniqueness`,
`cst_sc_range_consistency`, `sc_code_not_in_arangodb`) and report per file
whether its fragments are sound.

> **Trap: this list cannot be replaced by a computed predicate, and trying is a
> data-quality defect.** Coverage says nothing about correctness — a
> confidently wrong parse has every `sc_code` in place and no gaps. Deriving the
> set from "every fragment is mapped and the spans are gapless" admits fragments
> from files nobody has checked.

> **Trap: `frag_review` is not a verdict and cannot reconstruct the list.**
> `checked` means a human eyeballed that fragment so the parser could move past
> a stuck or edge-case state; its **absence** means the fragment never needed
> scrutiny, not that it is wrong. Measured over the shipped database: 155 of
> 7,349 Sutta fragments carry any review value, across 23 files; **no file** has
> all of its fragments marked, not even a verified one; and some unverified
> files carry more marks than verified ones. The validation verdict is never
> written back into `fragments.sqlite3`, so the list is the only record of it.

The list was previously called `CHECKED_XML_WHITELIST` and lived in
`tipitaka_xml.rs`. It is the same 30 entries; the name changed because
"checked" collided with `frag_review = 'checked'`, which means something else.

**To add a file:** run the parser's validations over it, review the output, add
it to the list, then re-run `import-cst`, `build-navigation-tree` and
`index rebuild --area suttas --lang pli` (see §6).

### 3.2 How completely do the fragments cover it? — the coverage class

Computed per file from `fragments.sqlite3`, and applied **only within** the
verified set.

| Class | Predicate | Records |
|---|---|---|
| **fully mapped** | verified, every `frag_type='Sutta'` fragment has an `sc_code`, and they leave no interior gap | fragments only |
| **partially mapped** | verified, some but not all fragments mapped | divisions **plus** the mapped fragments |
| **unmapped** | verified, none mapped | divisions only |
| **unverified** | not on the list | divisions only |
| **unsplittable** | the division split failed its cross-check | the whole-file record |

Within the verified set the class is computed, so adding mappings to an
already-verified file moves it between classes with no code change.

**Full** coverage is the predicate, not "has any `sc_code`": the weaker test
would promote a file with 2 of 33 fragments mapped to fragment-only import and
lose the other 31. **Gaplessness** is part of it for the same reason — a file
whose fragments are all mapped but leave an interior hole does not cover its own
text.

### 3.3 Why a coarser record is dropped where a finer one covers the file

The same Pāli text is never stored twice. Duplication is a **search defect**
before it is a size problem: the whole-file records were 74 MB of the `suttas`
table's 163 MB of `content_plain`, carried by a *trigram* FTS5 index and an
841 MB Tantivy index, and a duplicated text returns two hits for every query
that matches it.

The accepted exception is the partially mapped class, whose mapped portion is
stored twice — as a division and as a fragment — so that the *unmapped*
remainder stays findable. That is a few MB across four files today.

### 3.4 Dual identity, in the partially mapped class only

A partially mapped file's division rows and mapped fragment rows are **sibling
rows under one parent**, in source order — not a second line on a division's
row, and not nested inside it. Both are independently openable. Every fragment
row carries `cst_file`, so the window can mark it as a portion of that file and
it is not mistaken for a duplicate of the division above it.

## 4. Splitting a source file into the toc's divisions

For every file that is not fully mapped, the bootstrap cuts `romn/<name>.xml`
into one record per leaf of `tipitaka.org/romn/cscd/<name>.toc.xml`, named to
line up with the leaf (`s0305m.mul11.xml/pli/cst`).

**The chapter markup gives the boundaries; the toc leaf count decides whether
the head before the first marker is a division of its own.** Measured across all
217 files: 169 have exactly as many leaves as `rend="chapter"` markers, 47 have
one more, and one has one fewer.

> **Trap: the leading division cannot be read from the markup alone.** Deciding
> it by "body text precedes the first chapter marker" matches only 205 of 217
> files, whatever the definition of body text: eleven files open with real
> preamble gāthās that the toc **folds into the first division** rather than
> listing separately. When the counts are equal that head is prepended to
> division 0 — never dropped, and never made a division the toc has no leaf for.

A file whose counts differ by anything else is reported and imported whole
instead (§5), so no text is ever lost. On today's data that is one file,
`abh05t.nrf.xml`.

Each piece is **re-balanced**: any element still open at a boundary is closed at
the end of the outgoing piece and re-opened in the incoming one, so every
division is well-formed on its own. A `<div>` opened immediately before the
marker with only whitespace in between is trimmed off rather than closed as an
empty element — that is the common shape, and leaving it in ends each division
with an empty div belonging to the next one.

### 4.1 Toc leaves of a fully mapped file become branches

Such a file produces no division records, so each of its toc leaves is a
**branch node whose children are the fragments falling under it**. That keeps
the finest granularity available without storing the text twice.

The join is on **titles**: a fragment falls under the leaf whose title matches
its own `cst_sutta` or any of its group levels' titles, niggahita-folded.
Matching group levels alone is not enough — two Dīgha suttas have an empty
sutta-level group title.

**A toc leaf with no fragments of its own opens the adjacent record.**
SuttaCentral has no uid for a preamble or postamble section, so the corpus
merges such a section into the first or last sutta record of the file — by
design, and correctly. The CST toc still names the section, so its leaf points
at the record that carries the text: the next leaf's first fragment for a
preamble, the previous leaf's last fragment for a postamble.

Worked example: `s0301a.att.xml` lists `Ganthārambhakathā` as its first toc
leaf, and the `sn1.1` fragment begins with that section before running on into
the sn1.1 commentary. The leaf therefore opens `sn1.1.att/pli/cst`. These are
counted in the coverage report as "merged into an adjacent record" — they are
**not** unassigned leaves and **not** a defect.

A leaf with no fragments in a file that has none at all is a different matter,
and is reported.

**A leaf whose one fragment is the same text under the same name becomes a
leaf, not a branch.** Where the join yields exactly one fragment *and* its title
matches the toc leaf's (niggahita-folded), the leaf carries that fragment's
`sutta_uid` directly and has no child. The child would otherwise repeat the
parent verbatim — a row reading `1. Mahāpadānasuttavaṇṇanā` containing one row
also reading `1. Mahāpadānasuttavaṇṇanā` — and cost a tap for nothing.

**This skips no source level.** The level being dropped is not in the CST toc at
all; it is one the fragment join above invents, and a one-element group is not a
level. The equivalence test is what keeps that true: a leaf whose single
fragment has a *different* name is a real containment and keeps its branch.
Today 56 leaves collapse and 2 do not — `4. Okkantasaṁyuttaṁ` over
`1-10. Cakkhusuttādivaṇṇanā`, in the aṭṭhakathā and its ṭīkā, where the saṁyutta
really does contain one ranged commentary record.

The test compares the titles verbatim apart from the niggahita (the two strings
come from different sources and the corpus does not spell it consistently). Do
not loosen it — ignoring the leading number, say, would collapse genuine
containments.

> **Trap: several fragments can share one `sc_code`.** 110 do (three in
> `an4.1.att`), and the import keeps only the first. Anything building rows per
> *fragment* must de-duplicate by uid in the same order, or it names records
> that were never written and collides on `node_uid`.

## 5. The coverage report

`cli/src/bootstrap/navigation_tree/coverage_report.rs`, filled in two stages
because the halves are measured at different points: the **import** records each
file's class, the records produced, the files it could not split and the
`romn/` ↔ toc pairing in both directions; the **spine** records how many toc
leaves reached a record or a populated branch, and names the ones that did not.

Every figure is a count, so a regression is a number rather than a line to spot
in a long log. A count of zero still prints.

It is a report, never a repair: a non-zero unassigned count is a mapping to fix.

## 6. Rebuilding without a full bootstrap

Three commands, and **they must be run together** — the second reads what the
first wrote, and the third is not optional:

``` sh
simsapa_cli import-cst --db-path <appdata> --cst-xml-dir <tipitaka-xml> \
    --fragments-db-path <fragments.sqlite3>
simsapa_cli build-navigation-tree --db-path <appdata> --cst-xml-dir <tipitaka-xml> \
    --fragments-db-path <fragments.sqlite3>
simsapa_cli index rebuild --area suttas --lang pli --simsapa-dir <SIMSAPA_DIR>
```

`import-cst` deletes the existing `%/pli/cst` rows first: the importer skips a
uid that already exists, so without that a re-run would change nothing.

> **Trap: replacing the CST records invalidates two indexes and only one heals
> itself.** `suttas_fts` is kept in sync by its triggers. The **Tantivy index is
> not** — it keeps uids that no longer exist, and fulltext search silently
> returns stale hits until it is rebuilt.

`build-navigation-tree` takes `--sc-data-dir` and `--cst-xml-dir`
independently, so the CST spine can be rebuilt without a running ArangoDB (the
SC spine needs it for titles). Each spine build ends with an `ANALYZE` — see
[user-data-and-sqlite-analyze.md](./user-data-and-sqlite-analyze.md).

## 7. Related

- [database-migrations.md](./database-migrations.md) — the migration that
  creates `navigation_nodes`, and why a migration failure is non-fatal.
- [user-data-and-sqlite-analyze.md](./user-data-and-sqlite-analyze.md) — the
  `ANALYZE` that follows each spine write.
- `tasks/2026-09-18-174900-prd---sutta-navigation-window.md` — the PRD, whose
  FR-15 carries a dated correction about `VERIFIED_CST_FILES`.
