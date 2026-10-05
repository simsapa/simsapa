# Sutta navigation tree

The browsable hierarchy behind the Sutta Navigation window, built at bootstrap
time into the `navigation_nodes` table of `appdata.sqlite3`. The app never
assembles it at runtime and never queries ArangoDB for it.

The summaries and context notes a row shows are **not** in that table: they are
TOML files embedded in the app (§2f), so they change with an app update rather
than with a database download.

§1–§2 and §3–§6 are the bootstrap side: where the tree comes from and how its
rows are derived. §2a–§2g are what the window and its read layer
(`backend/src/navigation_tree.rs`, the `nav_tree_*` functions on `SuttaBridge`)
depend on. The window's lifecycle — one instance, kept across a close — is in
[window-lifecycle-and-reuse.md §5d](./window-lifecycle-and-reuse.md).

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

### 1.2 The SC grouping levels are collapsed where the Pāli-only scope empties them

The SuttaCentral super-tree's grouping levels exist to hold the **several
traditions'** collections side by side: `long` holds the Pāli `dn` beside the
Chinese `da` and `ma-ot`, and `vinaya` is split by school into `pli-tv-vi`,
`lzh-mg-vi`, `san-mg-vi` and the rest. Simsapa carries Pāli texts only, so the
scope allowlist leaves every one of those levels holding a single member, and
the level stops distinguishing anything. `Suttapiṭaka › Dīgha › Dīghanikāya`
asks the reader to walk two levels that say the same thing.

`collapse_redundant_groups()` in `cli/src/bootstrap/navigation_tree/sc_spine.rs`
elides them, so the tree reads:

```
Suttapiṭaka › Dīghanikāya › …          (not Suttapiṭaka › Dīgha › Dīghanikāya)
Suttapiṭaka › Khuddakanikāya › …       (not Suttapiṭaka › Khuddaka › Khuddakanikāya)
Vinayapiṭaka › Bhikkhu Vibhaṅga › …    (not Vinayapiṭaka › Theravāda Vinayapiṭaka › …)
```

**Two shapes, one redundancy.** A parent with exactly one child has a redundant
*edge*; which end survives depends on what the ends are.

| Shape | Example | Elided | Result |
|---|---|---|---|
| a grouping level with a single child | `sutta › long › dn` | the grouping | `sutta › dn` |
| a **root** whose single child is a grouping level | `vinaya › pli-tv-vi › …` | the grouping | `vinaya › …` |

Three rules keep it from taking the wrong level:

- **The order is shape 2, then shape 1** — and shape 2 asks its question of the
  tree **as built**. Reversed, `sutta › minor › kn` collapses first to
  `sutta › kn`, `sutta` momentarily has one grouping child, and the
  Khuddakanikāya's 20 collections get spliced into the root.
- **A root is never elided**, which is why the vinaya and abhidhamma cases give
  up the grouping instead of the root.
- **Only `group` nodes are elided** (see below). This is also the tie-break
  that keeps `kn` and drops `minor` — a rule phrased as "elide an only child"
  would take the wrong one.

**A vagga is never collapsed into a saṁyutta, and the `group` type test is what
guarantees it.** `sn8` Vaṅgīsasaṁyutta holds exactly one vagga,
`sn8-vangisavagga` Vaṅgīsavagga, and the shape is indistinguishable from
`sutta › long › dn`:

```
sn › sn-sagathavaggasamyutta › sn8 (Vaṅgīsasaṁyutta) › sn8-vangisavagga (Vaṅgīsavagga) › sn8.1 …
```

The difference is *why* there is one child. `long` holds one member because the
Pāli-only scope deleted its siblings — the level is an artefact. `sn8` holds one
vagga because the Saṁyutta Nikāya has one there. A saṁyutta and a vagga are
**distinct groupings of the original source with distinct names**, and a reader
may go looking for either *Vaṅgīsasaṁyutta* or *Vaṅgīsavagga*; collapsing them
into each other would lose a name the corpus uses and a `node_uid` it refers to.
**180 branch nodes in the built tree have exactly one child, and not one of them
may go.**

What makes the type test trustworthy is that `"group"` is assigned at exactly
two places — `build_groups()` and `build_group_children()`, the two super-tree
walkers. Nothing read from a `*-tree.json` can carry it: `build_collection()`
writes `"collection"`, `build_subtree()` writes `"branch"`, `make_leaf()` writes
`"sutta"`. Measured over the built spine: 3 `root`, 1 `group` (`kn`), 37
`collection`, 890 `branch` and 7,992 `sutta` rows — so the
elision can reach only the super-tree, and the one surviving `group` is the
Khuddakanikāya. **If a future change ever gives a `*-tree.json` node the
`"group"` type, this rule starts eating real levels of the canon.**
`a_samyutta_over_a_single_vagga_keeps_both_levels` in `sc_spine.rs` is the
regression test.

**Nothing of substance goes with them.** The elided levels carry the
cross-tradition blurb ("Collections of “long” discourses and other related
texts") while the surviving node carries the specific one ("The Long Discourses
(Dīgha Nikāya, abbreviated DN) is a collection of 34 discourses…");
`pli-tv-vi` and `pli-tv-ab` have no blurb at all, and their titles ("Theravāda
Vinayapiṭaka") restate the root's.

**Widening the scope undoes the collapse by itself.** The rule is computed from
the child counts the walk produced, not from a list of levels to skip, so the
moment `SC_SCOPE` admits a second collection under `long` the level carries
information again and reappears — no code change. A unit test in `sc_spine.rs`
holds this.

**It is done at build time, not in the UI.** `parent_uid`, `path` and
`sort_path` are derived by the flattening pass from the collapsed tree, so the
level listing, the breadcrumb bar and the filter results' paths all follow with
no further code — which is what makes a filter hit read
`Suttapiṭaka › Saṁyuttanikāya › …` rather than
`Suttapiṭaka › Saṁyutta › Saṁyuttanikāya › …`.

**Node uids are not rewritten by it.** `kn` keeps the uid `sutta/minor/kn`
though its parent is now `sutta`: a uid is qualified by the node's ancestry **in
the source**, which is what makes it stable (§2). Were `minor` to regain a
sibling and stop being elided, `kn`'s uid would not move.

The CST spine is not touched. Its levels come from the toc files, which describe
one tradition already and have no tradition-splitting level to lose.

### 1.3 Shapes that look like a failed build and are the source's own

The tree is depth-agnostic: every level is whatever `parent_uid` says, never a
fixed number of levels per collection. So a collection whose texts sit directly
under it, or a level labelled differently from its neighbour's, is the source's
shape and not a structure the builder failed to derive. The ones that look most
like defects:

- **Flat collections on the SC spine.** In the Khuddakanikāya `kp`, `dhp`,
  `bv`, `mnd` and `pe` hold their texts directly, while the other fifteen
  members nest (`thag` 21 branches, `tha-ap` 56, `ja` 22). All seven Abhidhamma
  books are flat, which is where the tree's largest level comes from: `patthana`
  with 728 rows, then `kv` with 219. They are flat in `sc-data`'s own
  `*-tree.json`. An **empty** KN member, by contrast, is a build defect — all
  twenty have Pāli rows.
- **`dhp`'s 26 children are texts, not vaggas to enter.** SuttaCentral stores
  the Dhammapada one vagga per document (`dhp1-20` Yamakavagga), so the vagga
  *is* the leaf.
- **Añña is properly structured.** `toc4.xml` holds nine titled groups
  (Visuddhimagga, Saṅgāyana-puccha vissajjanā, Leḍī sayāḍo, Buddha-vandanā,
  Vaṁsa, Byākaraṇa, Nīti, Pakiṇṇaka, Sihaḷa), each over `src=` books whose
  leaves are divisions. Nothing there needed a flat-list fallback.
- **Every `cscd/*.toc.xml` is flat** — one root `<tree>` with its leaves under
  it, in all 217 files. The leaf walk recurses regardless, so a nested toc
  would build rather than be truncated.
- **A CST level is labelled from its own title first, and from its depth only
  when the title names no level.** `level_label()` in `cst_spine.rs` matches the
  title's stem (`saṁyutt`, `paṇṇāsa`, `vagg`, `nikāy`, `sutt`) and falls back
  to `NikayaStructure`'s level list for a title such as `Ganthārambhakathā`.
  Depth alone mislabels the commentaries, whose tocs are shallower than the
  mūla they comment on: the Aṅguttara aṭṭhakathā's leaves are vaggas sitting
  at the depth the mūla structure reserves for a paṇṇāsaka. The vocabulary is
  `GroupType`'s either way; the SC spine's levels keep SuttaCentral's names
  (§1).
- **The Vinaya's three layers are under three roots and are not joined.** A
  rule's mūla is on the SC spine, its aṭṭhakathā under `cst:atthakatha` and
  its ṭīkā under `cst:tika`, in three files with three internal structures.
  There is no cross-root "related texts" row.

## 2. Node uids are stable across bootstraps

The UI persists expansion state and the breadcrumb path by `node_uid`, so it is
derived from source identifiers only — never from a row id, never from an
enumeration counter.

| Node | `node_uid` |
|---|---|
| SC grouping level | qualified by its ancestors: `sutta`, `sutta/minor/kn` (kept even when §1.2 elides the ancestor from the tree) |
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

### Where an SC leaf's titles come from

The English line is filled from the first of these that has a non-empty value:

1. `title_trans` on the row the leaf opens — the corpus's own translated title
   for that exact text;
2. the ArangoDB `names` collection (`lang = "en"`);
3. an English row's `title`, preferring `/en/sujato`, then `/en/brahmali`, then
   any other translator (`english_title_rank()`).

The Pāli line is `names` (`is_root`) first, then the Pāli row's title. **An
empty `names` entry counts as missing** — `make_node` filters it — or it would
stop the fallbacks from ever running (`sn47.4` has an empty English name).

The fallbacks are keyed by the leaf's **own** uid, so a redirected leaf never
borrows its parallel's title. Leaves with no English title after all three
(1,711 today: `patthana`, `tha-ap`, `thi-ap`, parts of `ds`/`ya`) have English
rows only with an empty `title`; there is nothing to fall back to.

> **Trap — Thanissaro's rows store the two titles swapped:** `title` holds the
> Pāli name (`Sālā Sutta`) and `title_pali` the English one (`At Sālā`). Tier 3
> reads `title`, so where Thanissaro is the only English row with a title, the
> English line shows the Pāli name. One leaf is affected today (`sn47.4`). This
> is a source-data defect to report, not one to work around here.

### The sutta reference before a leaf's title

A leaf's headline is `<sutta_ref> <title>` — `SN 56.11 Dhammacakkappavattanasutta`
— the same reference the sidebar's search results show. It is **not stored in
the tree**: `attach_sutta_refs()` reads `suttas.sutta_ref` for a whole level in
one query when `children_json` / `filter_json` / `node_json` serialize it, so it
always matches the row the leaf opens. `wants_sutta_ref()` leaves it off a
redirected leaf (the ref would be its parallel's) and off a CST division or
whole-file record (its stored ref is derived from the file name, e.g.
`Vin 01a.att0`). The filter does not match on the stored ref; a typed reference
is recognised instead (next section).

### A filter query typed as a reference

`ref_query_uids()` reads a query such as `dn 1`, `SN 56.11` or `Dhp 182` as
uids, and `filter_nodes_conn` lists the nodes those uids name **exactly** first
(`exact_uid_nodes_conn`: the node's own uid, or a leaf whose `sutta_uid` is
`<uid>/…`, never through a redirected leaf's borrowed text), then the substring
matches. Two readings, both kept, literal first:

1. the reference with its spaces removed, via `whole_sutta_ref()` in
   `helpers.rs` (`sn 56` → `sn56`, `SN 56:11` → `sn56.11`, `Dhp 1-20` →
   `dhp1-20`). The query is folded with `normalize_for_search` first, so
   `Paṭṭhāna 1` reads as `patthana1`. The Theragāthā and Therīgāthā short
   forms (`Th`, `Tha`, `Thi`, `Thī`, `Thīg`) are rewritten to `thag` / `thig`
   first, by `canonical_thag_thig()`, which the search bar shares;
2. the search bar's own reading in the **Suttas** area,
   `query_text_to_uid_field_query(query, &SearchArea::Suttas)`. It converts
   verse numbers, so `sn 56` also yields `snp1.3` (Suttanipāta verse 56) and
   `Dhp 182` yields `dhp179-196`.

The substring pass runs on the reading that named a node. Failing that, on each
reading in turn (`kv 1` names no node, but `kv1` prefixes `kv1.1`, `kv1.2`, …),
and last on the text as typed, because a reading that matches nothing
(`raja 1` → `raja1`) was never a reference.

## 2b. The breadcrumb bar shows the path, never the history

`SuttaNavigationWindow.enter_level()` recomputes the whole path from the node's
own ancestry (`ancestor_segments()`, which walks `parent_uid`) rather than
appending a segment to the path the user happened to walk.

Appending is wrong because a **tree row** can sit several levels below the
current one: FR-33's expand-in-place reveals descendants at any depth, so
expanding Suttapiṭaka, then Majjhimanikāya, and entering Mūlapaṇṇāsa produced
`All › Mūlapaṇṇāsa` — a record of which buttons were pressed, not a statement of
where the reader is. It must read
`All › Suttapiṭaka › Majjhimanikāya › Mūlapaṇṇāsa`.

Recomputing also collapses the three routes into a level — a level row, a tree
row at any depth, and a flat filter hit whose ancestors are nowhere on the
current path — into **one** function, so the bar cannot say different things
about the same level depending on how it was reached. For a level row it
produces exactly what appending did, which is why the level-row case never
showed the defect.

`go_to_breadcrumb()` slicing the array stays correct precisely because the array
is now a real path: segment *i* is the ancestor at depth *i*.

## 2c. A textless leaf is redirected through its parallels

Some SuttaCentral leaves have **no document of their own**. The Pātimokkha is
the rules as they are *recited*, and SuttaCentral gives each rule its own leaf —
`pli-tv-bu-pm-pj1` — but renders that page as a list of the same rule in every
tradition it carries (`lzh-dg-bu-vb-pj1/lzh/taisho`, `xct-mu-bu-pm-pj1`, …).
Among them the Pāli one is `pli-tv-bu-vb-pj1/pli/ms`: the rule inside the
Vibhaṅga, with its analysis. Simsapa carries only the Pāli, so the parallels
page has nothing to show and the leaf should simply open that text.

`ScSpineBuilder::parallel_redirect()` does this, from SuttaCentral's own
`relationship/parallels.json`, and the five leaves that stand for a *span* of
rules are expanded into them (§2d). Measured: **543 of 543 — the SC spine has
no dimmed leaf left, and no leaf without a text**, with zero ambiguities. Left
unresolved they made **both Pātimokkhas unbrowsable** — their collection nodes
are branches with no text of their own, so every route into them ended on a
dimmed row.

### Four tiers, own division first

A parallels group is *the corresponding rule across traditions **and across the
two divisions***, so every vinaya group holds four Pāli members — the bhikkhu
pair and the bhikkhunī pair:

```
pli-tv-bi-pm-ss7 → { pli-tv-bu-vb-ss5, pli-tv-bu-pm-ss5,
                     pli-tv-bi-vb-ss7, pli-tv-bi-pm-ss7 }
```

Both `pli-tv-bi-vb-ss7` and `pli-tv-bu-vb-ss5` are legitimate answers, but not
equally: the nun's own rule is her text, and the monk's is what SuttaCentral
falls back to when the nuns' Vibhaṅga says the rule is the monks'. So the
resolver walks four tiers and stops at the first that yields one candidate:

| Tier | | Resolved |
|---|---|---|
| 1 | own division, a row of its own | 345 |
| 2 | own division, **inside a ranged row** | 26 |
| 3 | other division, a row of its own | 335 |
| 4 | other division, inside a ranged row | 0 |

**706 redirects, 0 ambiguities.** Both Pātimokkhas go from *entirely
unbrowsable* — their collection nodes are branches with no text of their own —
to 311/311 and 227/227 openable. Tier 3 grew from 167 to 335 when §2d's
expansion added the 168 bhikkhunī Vibhaṅga rules, which resolve by this same
path with no special case.

Two guards keep the cross-division tiers honest:

- **A candidate must itself be a rule**, i.e. have a division (four or more
  hyphen segments). The groups also name whole Khandhaka chapters
  (`pli-tv-kd22`), where a rule's story is told rather than its text; admitting
  them left five leaves ambiguous between the chapter and the rule.
- **`pli-tv-` is the whole reach of the mechanism.** That is SuttaCentral's
  Theravāda Vinaya prefix; a Pāli *sutta* uid is `mn10`, with no prefix, so a
  textless leaf outside the vinaya can never be redirected and stays an honest
  dimmed row. Every one of the 543 is a vinaya rule, so nothing is given up by
  refusing to generalise past what was measured.

**The cross-division tier was corroborated independently: all 167 of its
redirects have a Pāli title *identical* to their target's** — Sañcaritta →
Sañcaritta, Saṅghabheda → Saṅghabheda — which is evidence the group names the
same rule rather than a merely related one.

### Ranged rows reuse the sutta range parser

SuttaCentral names a rule individually (`pli-tv-bi-vb-pj1`) where the corpus
stores several as one document (`pli-tv-bi-vb-pj1-4`, the four pārājikas the
nuns share with the monks). Tier 2 finds those with
`helpers::sutta_range_from_ref()` — **the same parser the sutta import uses for
`sn30.7-16`**, which handles vinaya rule uids unchanged:

```
pli-tv-bu-vb-as1-7   → group "pli-tv-bu-vb-as", 1..=7
pli-tv-bi-vb-sk2-74  → group "pli-tv-bi-vb-sk", 2..=74
```

There is no second range parser here. The index it builds keeps **only rows
spanning more than one number**: a single-number row is already reachable by
tiers 1 and 3, so indexing it would let a range tier re-find it and make
"resolved through a range" mean less than it says.

Tier 2 sitting above tier 3 is what keeps `pli-tv-bi-pm-pj1` on the nuns'
`pli-tv-bi-vb-pj1-4` instead of reaching across to the monks'
`pli-tv-bu-vb-pj1`, which is also in its group.

### Why the uid is not simply rewritten

Replacing `-pm-` with `-vb-` reproduces 342 of tier 1 and never disagrees with
the data. It is still the wrong implementation: it is a guess about uid spelling
with nothing to check it against, and it is incomplete — `pli-tv-bi-pm-pc91`,
`pc92` and `pc93` are all one text, `pli-tv-bi-vb-pc90`, which no rewrite finds,
and it reaches neither the ranged rows nor the cross-division rules, which are
193 of the 538. The group states the relationship; the code only reads it.

Two further rules:

- **Only the `parallels` relation is followed.** The file also carries
  `mentions` (2,497 groups) and `retells` (62). Those say one text refers to
  another, which is no reason to open it in place of the text the reader asked
  for.
- **A tie is reported, never guessed.** Where two candidates survive a tier
  (`pm-pj4`'s group also names `vb-pc8`), the rule designator at the end of the
  uid breaks it — a tie-break *among candidates the source already vouched
  for*, not a way of finding one. If it does not single one out, the leaf stays
  dimmed and the candidates are logged. Zero such cases today.

### The row says where its text comes from

**This is not optional.** A bhikkhunī rule opening a bhikkhu rule with nothing
to explain it is a reader left wondering whether the app is broken. So
`navigation_nodes.redirect_uid` stores the **target's own uid**, and
`NavigationNodeRow.qml` renders it as a **badge**:

```
Sañcaritta
uid: pli-tv-bi-pm-ss7       ┃ Parallel: pli-tv-bu-vb-ss5 ┃
```

A badge, not grey italics, and **exempt from the Summaries toggle** (§2e): this
is the one thing on the row that contradicts what the title promises, so it has
to be the thing that is seen, and it is not reference detail that a compact view
may drop. It is tinted from `palette.highlight` so it reads in both themes —
the idiom `StorageCandidatesList.qml` already uses. FR-17's "in `<file>`"
fragment note is exempt for the same reason.

Two details that are easy to get backwards:

- The uid label shows the row's **own** `node_uid`, not `sutta_uid` as an
  ordinary row does. The uid is how a reader tells this nuns' rule from the
  monks' rule of the same Pāli name, so borrowing the target's uid there would
  erase exactly the distinction the label is needed for.
- `redirect_uid` holds the target's own uid (`pli-tv-bu-vb-ss5`), not the
  `suttas.uid` (`pli-tv-bu-vb-ss5/pli/ms`) that `sutta_uid` holds. The row shows
  it to a person, and the edition suffix is noise there.

The column arrived in its **own** migration,
`2026-09-22-000000_navigation_node_redirect`, rather than as an edit to
`2026-09-19-000000_navigation_tree`: that one is already stamped in the
`__diesel_schema_migrations` ledger of every database built since, so an edit
would never re-run. `backend/src/navigation_tree/tests.rs` applies **both**
migration files, and a new one must be added to that list or the read-layer
tests start passing against a schema no installed database has.

**Splitting the schema across two migrations made a new state reachable, and it
had to be guarded.** A migration failure at startup is non-fatal by design, so
"table present, `redirect_uid` missing" is a database a user can really have.
`tree_is_available_conn()` used to answer with `COUNT(*)`, which **succeeds** on
exactly that table — the window would be told the tree is available and then
every level query would fail, giving an empty window with no explanation, which
is the outcome FR-13 exists to prevent. It now selects a whole row, so it fails
the same way the level queries will and the window shows its honest "navigation
index not available" message. `a_table_missing_a_later_migrations_column_is_not_available`
is the regression test, and the rule generalises: **anything that answers "is
this table usable?" must touch the columns the real queries name.**

## 2d. A leaf that stands for a span of rules becomes a branch over them

Five leaves of the bhikkhunī Vibhaṅga name *several rules at once* —
`pli-tv-bi-vb-ss7-9`, `ss14-17`, `np13-30`, `pc97-166`, `sk2-74` — because the
nuns' text says only that those rules are the monks'. There is no document
behind the ranged uid and none behind it in this corpus either, so §2c's
redirect found nothing and **168 rules were unreachable** behind five dimmed
rows.

Each rule *individually* does resolve. `pli-tv-bi-vb-ss7` is in
`parallels.json`, and its group names `pli-tv-bu-vb-ss5`. So
`ScSpineBuilder::expand_range_leaf()` turns the range into a **branch** and each
rule into a leaf, resolved by the ordinary path — §2c needs no special case for
them, and they pick up their titles and blurbs like any other leaf:

```
Saṅghādisesa
  …
  pli-tv-bi-vb-ss7-9                      ← branch, 3 children
      Sañcaritta   7. Matchmaking         text: pli-tv-bu-vb-ss5
      Duṭṭhadosa   8. Making a groundless accusation out of anger
      Aññabhāgiya  9. …using a pretext
```

Measured: **all 168 numbers are uids SuttaCentral names, and all 168 resolve**
(every one at tier 3). 95 of them even have a blurb.

### Three guards, and each one is load-bearing

**Only unresolved leaves reach here.** Expansion is the last arm, after "has its
own text" and "has a parallel". The SC spine holds **261 other ranged leaves** —
every SN, AN and Dhp range (`sn30.7-16`, `an1.394-574`, `dhp1-20`) — and the
corpus stores each under that very uid, so not one of them reaches the
expansion. Exactly five do. Getting this wrong would shatter the Aṅguttara into
thousands of nodes for texts that do not exist.

**Every rule uid must be one SuttaCentral names.** The numbers come from the
range the source itself wrote, but a uid built that way is still a
*construction*, so each must appear in `parallels.json` before it may become a
node. One unknown number and the whole range is left alone and reported —
sprouting rules SuttaCentral has never heard of is precisely the invention FR-8
forbids.

**At least one rule must have a text**, checked *before* the children are built.
One honest dimmed leaf beats a branch of 73 dimmed ones, and building first
would record 73 defects against rows that are then thrown away rather than one
against the range.

A rule that is named but has no text is still kept and dimmed, like any other
leaf: the gap is reported where it is rather than hidden by dropping the row.
Recursion terminates because a single-number uid has `start == end` and returns
on the first check.

**The watch item: a collision with a sibling.** The expansion mints
`pli-tv-bi-vb-ss7`, and the bhikkhunī Vibhaṅga tree currently lists `ss1`–`ss6`,
`ss7-9`, `ss10`–`ss13`, `ss14-17` — so no minted uid is also a leaf of its own.
If SuttaCentral ever lists **both** a range and one of its members, the two
nodes collide on `node_uid`, which is `UNIQUE`. `flatten()` already handles it
without corrupting anything: the first occurrence wins, the second is recorded
as a defect and **its whole subtree is skipped**. So the failure is visible in
the defect counts rather than silent — but it is a reason to read those counts
after an `sc-data` refresh, not only the row total.

### The expanded branches have no title of their own

`pli-tv-bi-vb-ss7-9` has no entry in the `names` collection, none in
`uid_expansion.json` and no blurb, so the branch falls back to displaying its
uid — which is legible (`ss 7-9`) and is what the leaf displayed before it
became a branch. **No source names it, so none is invented.** If a label is ever
wanted there, it has to come from a source, not from composing the children's
titles.

## 2e. The Summaries toggle governs every row

The top bar's checkbox is labelled **Summaries** and controls the blurb **and
the uid**, on **every** row in the window — level rows, tree rows and filter
hits alike. On means on for all of them; off means off for all of them.

It did not start that way, and the first reading was a defect worth recording.
The PRD scoped the toggle to *level rows*, so a tree row never showed a blurb
"whatever the toggle says". In use that reads as a **broken checkbox**: the box
is ticked and all but a handful of rows on screen show nothing, with no way for
a reader to tell that the rule is about row modes rather than a fault. The
compact view the rule was protecting is exactly what the toggle is *for* — so
the reader chooses it, rather than having it imposed per row mode.

What still distinguishes a **tree row** is that it omits the child count and the
translation hint, and that it indents.

### Three naming and scope decisions

- **"Summaries" in the UI, `blurb` in the data.** `blurb` is SuttaCentral's own
  term and is what `blurbs.json`, the `assets/navigation-blurb-*.toml` files,
  the row's JSON field and the persisted setting key hold; the label is for readers, who should not have to
  learn it. The bridge functions stay `get_show_navigation_blurbs` /
  `set_show_navigation_blurbs`: renaming the stored key would silently reset
  everyone's preference, and a bridge API change carries its own hazard (see
  docs/qmltypes-aot-staleness.md).
- **The uid follows the toggle.** A uid is reference detail of the same kind as
  a summary, so hiding one while keeping the other gives a compact view that is
  barely more compact. It is prefixed `uid: ` — an unlabelled `pli-tv-bi-pm-ss7`
  next to a badge and a file note is one opaque string among several — and drawn
  at **three-quarter-strength `palette.text`**, not `palette.mid`. `palette.mid`
  is a frame colour and leaves the uid almost invisible on some themes; full
  `palette.text` would let a reference string compete with the title. Deriving it
  from the palette is what keeps it correct in both light and dark.
- **The parallel badge and the "in `<file>`" note do not.** They say the row
  does not open what its title implies, which is not reference detail — a
  compact view that drops them is a view that lies. `meta_visible` in
  `NavigationNodeRow.qml` is the one binding that encodes this split; changing
  it to follow `show_summaries` alone is the regression to watch for.

## 2f. Summaries and context notes — embedded in the app, not in the database

A row can show two pieces of prose under its title: the **summary**
(SuttaCentral's blurb) and, below it, a **context note** (the project's own
text). Both are governed by the Summaries toggle.

**Neither is in `navigation_nodes`.** They live in three TOML files under
`assets/`, embedded in the app with `include_str!`
(`backend/src/app_settings.rs`), so a corrected summary or a new note ships with
an ordinary app update and needs no database download.

| File | Kind | Content |
|---|---|---|
| `navigation-blurb-suttacentral.toml` | **generated** by the CLI, never edited by hand | SuttaCentral's English blurbs, from `sc-data/additional-info/blurbs.json` |
| `navigation-blurb-overrides.toml` | **hand-written**, never written by the CLI | the project's corrections to the file above |
| `navigation-context-notes.toml` | **hand-written**, never written by the CLI | context notes |

All three use **one key scheme**, the quoted `node_uid`, and two shapes. The
two blurb files are a flat table, each value a string that can be wrapped
freely:

``` toml
# Dhammacakkappavattanasutta
"sn56.11" = """
Lines are joined with a space, so a text
can be wrapped wherever it reads well.

A blank line starts a new paragraph.
"""
```

The context notes are **one table per node**, because a note also carries the
dates that place the node on the Timeline page (§2h):

``` toml
["dn"]
from = -500
to = -200
note = """
Origin: 5th–3rd century BCE, northern India. …
"""
```

| Field | | Meaning |
|---|---|---|
| `note` | required | the text shown in the row, wrapped like a blurb |
| `from` | optional integer | earliest year of origin; **BCE is negative** (`-500` is 500 BCE) |
| `to` | optional integer | latest year of origin |
| `circa` | optional, default `false` | the bounds are approximate |

Only `from` means "not before", only `to` means "not after", and an entry with
neither is undated. There is no year 0. Centuries are round numbers ("5th–3rd
century BCE" is `-500` … `-200`, "12th century CE" is `1100` … `1200`); the
file's header comment carries the whole convention, with a line per wording.
**The note text is never generated from the numbers** and the numbers are never
read out of the text: they are two statements of the same date, kept in step by
hand. A back-reference note ("Origin: See Dīghanikāya.") carries its own copy
of the fields of the entry it refers to.

Two parsers in `backend/src/navigation_texts.rs`, used by the app and by the
CLI alike: `parse_node_texts()` for the blurb files and `parse_context_notes()`
for the notes. The text shown is the **unwrapped** text (`unwrap_note()`): lines
joined with one space, runs of whitespace collapsed, a blank line kept as a
paragraph break.

- **The key is the `node_uid`, not the `suttas.uid`.** `sn56.11`, not
  `sn56.11/pli/ms`; a CST node is `cst:…`; a super-tree level is qualified by
  its ancestors (`sutta/minor/kn`). `SELECT node_uid, title_pali FROM
  navigation_nodes` finds one, and so does the generated file, where every
  entry is preceded by the node's title.
- **Always quote the key.** Unquoted, TOML reads `sn56.11 = "…"` as the table
  `sn56` holding the key `11`, and `[sn56.11]` the same way. That shape, like
  anything else that is not the file's own shape, is a parse failure for the
  whole file.
- **A context note with a field the app does not know is a parse failure**
  (`deny_unknown_fields`), so `form = -500` cannot be dropped silently and
  leave the node undated.

### Which text a row shows

`NavigationTexts` (`navigation_texts.rs`) holds the three parsed maps and
resolves by `node_uid`:

1. an entry in the **overrides** file wins — and if it is empty, the row has no
   summary;
2. otherwise the **SuttaCentral** file's entry, if any;
3. otherwise none.

So an override does one of three things:

| Override | SuttaCentral has an entry | Shown |
|---|---|---|
| text | yes | the override (**replace**) |
| text | no | the override (**add**) — any node, including a `cst:` one |
| `""` | yes | nothing (**suppress**) |

The context note is the notes file's entry, if any.

The texts are attached where a row's JSON view is built
(`NavigationNodeView::from_node(node, texts)`), for all three query routes —
a level's children, filter hits, a single node — under the field names `blurb`
and `context_note`. The view takes the texts as an **argument** rather than
reaching for the global, so a view test supplies inline TOML and does not depend
on the shipped files.

### When the files are parsed

Once per process, on the window's first query — never at startup.
`navigation_texts()` is a `OnceLock`; its only callers are the three `*_json`
functions, a warm-up, and the Timeline page's route (§2h), which runs on a
Rocket worker thread. The warm-up exists because the window's first query is
a synchronous call on the GUI thread: `SuttaBridge::open_sutta_navigation_window()`
starts the parse on a background thread before it hands over to the window
manager, and a query that arrives early waits for that one initialization
instead of starting a second. The parse time and the three entry counts are
logged once, at `info`.

> **Do not move the parse to startup**, and do not warm it from
> `AppData::new()`, `init_app_data()` or a `Component.onCompleted` of
> `SuttaSearchWindow` — see
> [startup-sequence-and-caches.md](./startup-sequence-and-caches.md).

**A file that fails to parse does not break the window.** It is logged once,
naming the file, and treated as empty; the other two are still used and the
tree itself works regardless. That is the last resort: a `backend` test
(`the_embedded_files_parse`) fails the test run on an unparsable embedded file,
and the tree build fails outright on an unparsable hand-written one.

### Editing a hand-written file

Edit the file and **rebuild the app**. No bootstrap run and no database change
is involved.

**A bad key is reported by the tree build**, not by the app. Every SC or CST
spine build (the full bootstrap and `build-navigation-tree`, §6) checks the keys
of the two hand-written files against the tree it has just built —
`validate_node_text_keys()` in `cli/src/bootstrap/navigation_tree/mod.rs` — and
records in the spine's defect summary:

| Defect | Meaning |
|---|---|
| `context note for a node uid not in the tree` | a notes key names no node |
| `blurb override for a node uid not in the tree` | an overrides key names no node |
| `empty blurb override for a node with no SuttaCentral blurb` | a `""` that suppresses nothing |
| `context note with from later than to` | the two bounds of a note are the wrong way round |

It reports and does nothing else: no key is dropped, renamed or guessed, and
none fails the build. The two spines are built independently, so each checks
only the keys it owns — a `cst:` key is the CST spine's, everything else the SC
spine's — and a typo is therefore visible only in the log of the spine that owns
it. The build reads the copies of the two files **embedded in the CLI**, so
rebuild the CLI after an edit before relying on its report.

### The generated file

`cli/src/bootstrap/navigation_tree/blurb_export.rs` rewrites
`navigation-blurb-suttacentral.toml` on every SC spine build. It holds **one
entry per tree node that has a blurb, keyed by that node's `node_uid`, and
nothing else** — in tree order, each entry preceded by a comment with the
node's title, the text wrapped at a fixed width, and no timestamp, path or
count anywhere, so the same sources give a byte-identical file and a
re-bootstrap leaves `git status` clean.

The text is SuttaCentral's, trimmed. The exporter does not rephrase,
re-punctuate or strip markup.

Every `blurbs.json` entry is accounted for as exactly one of three things, and
the lists behind the last two are **fixed data in the code**, each with its
reason beside it:

- **used by a node** of the final tree — after the single-member grouping
  levels have been collapsed (§1.2), so an elided level is not a node;
- **re-keyed** by `BLURB_KEY_REMAPS` — blurbs SuttaCentral files under a uid its
  own tree does not use, written under the `node_uid` they belong to (a range
  the tree splits in two gets the text on both nodes). The entry's comment
  names the source key, e.g. `# Vātasutta (blurbs.json key: sn23.159)`. It is a
  list of named cases: **nothing is inferred** — no range arithmetic, no
  nearest uid;
- **skipped** by one of three lists — the super-tree grouping words, the
  collections outside the Theravāda Pāli scope, and the per-rule Vibhaṅga
  blurbs, which have no node of their own and repeat the Pātimokkha rule's
  text.

Anything that fits none of these is reported, not written, and never forced:

| Defect | Meaning | What to do |
|---|---|---|
| `blurb for a uid the tree has no node for` | an entry no node uses, not re-keyed and on no skip list — new upstream, or newly mis-keyed | add it to a skip list or to the re-keying table |
| `blurb skip list: key not in blurbs.json` | a stale skip-list row | delete the row |
| `blurb skip list: key is now used by a node` | a stale row; the node gets its blurb as usual | delete the row |
| `blurb skip list: Vibhaṅga text differs from its Pātimokkha counterpart` | the "redundant" reason no longer holds for that rule | decide where the text belongs |
| `blurb key remap: source key not in blurbs.json` | SuttaCentral removed or renamed the key | delete the row |
| `blurb key remap: target node not in the tree` | the tree no longer has that node | fix or delete the row |
| `blurb key remap: target node has its own blurb` | SuttaCentral fixed it; the node's own entry wins | delete the row |
| `blurb key remap: prefix not found` | the text no longer starts with the stated prefix | fix the row |

A rejected re-keying row writes nothing, so its blurb is then also reported as
used by no node.

The write is all-or-nothing: only after `blurbs.json` has loaded with at least
one English entry **and** the spine has been written to the database, to a
temporary file that is renamed into place. In every other case the existing
file is left untouched and a warning says why. It is written only into a folder
that already holds `navigation-context-notes.toml`, so a wrong working
directory never creates a stray file.

> **The embedded copy is one build behind a regeneration.** The run rewrites
> the file on disk; the CLI that wrote it and any existing app binary still
> embed the previous one. **Rebuild the app** for a changed file to take
> effect — the build log says so when the file changed. The key validation is
> unaffected: it uses the blurbs of the tree just built, not the embedded file.

The file is committed: `include_str!` fails the build without it, and the CLI
that generates it depends on `backend`.

### The context notes

The notes written so far give each collection's **origin**: the time period,
the country, the probable author, and how well anchored the date is. The
evidence and reference URLs behind every one are in
[navigation-context-notes-research.md](./navigation-context-notes-research.md);
a note is refined there first, then in the TOML file.

In the row the note comes after the summary, set apart by a divider line that
is drawn only when the row has both. It is truncated to two lines like the
summary but has **its own** more/less button and its own expansion state
(`context_note_expanded_uids` in `SuttaNavigationWindow.qml`), so opening one
does not open the other. Neither text is part of `search_text`, so the filter
does not match on them.

## 2g. The window: what its rows are built from

`SuttaNavigationWindow.qml` is a drill-down, not a mega-tree. **Entering** a
level replaces the list with that node's children at zero indentation;
**expanding** a row in place reveals its descendants as indented tree rows, for
a look inside a branch that is then flicked shut. A tap on a row body does the
one thing the row is for — a branch expands, an openable leaf opens — and
entering a level or reading more of a summary is always a button. No row is
both a branch and a leaf, so a tap is never ambiguous on a touch screen.

The rules below are the ones that break quietly.

- **The level list's model is a `ListModel` edited in place, never a JS array
  that is reassigned.** Assigning an array resets the `ListView`: it jumps to
  the top and rebuilds every delegate, and a rebuilt row settles its height
  only after its first layout pass, so the scroll position cannot be restored
  afterwards. `toggle_expand()` inserts and removes below the toggled row and
  leaves every row above it untouched.
- **Per-row state lives in the window, keyed by `node_uid`** —
  `expanded_uids`, `blurb_expanded_uids`, `context_note_expanded_uids`. A
  `ListView` recycles delegates, so a flag kept in the delegate lands on
  whichever row scrolls into it next.
- **The level and the filter hits are two `ListView`s, not one with a switched
  model.** The level list is only hidden while a filter is active, so clearing
  the filter returns to the same level at the same scroll position with no
  restore code.
- **A filter hit carries its ancestors' titles, not their uids.** Entering a
  level from one walks `parent_uid` up through `nav_tree_node_json()`
  (`ancestor_segments()`), which is the same walk §2b uses for every other
  route.
- **Translation counts are one query per level and are not cached.** The count
  is "translations in the languages installed right now", so it changes with a
  language download. `translation_counts_conn()` asks for the whole level at
  once as indexed prefix ranges (`uid >= 'sn56.11/' AND uid < 'sn56.110'`,
  never `LIKE 'sn56.11%'`, which would fold `sn56.110` in), excluding the root
  language. The window re-reads them in `onVisibleChanged`, because it is
  hidden rather than destroyed and a language can arrive while it is away.
  Only level rows are counted; a uid with no translation is absent from the
  map and draws no indicator.
- **Two reads are posted with `Qt.callLater`**: the first level query, so the
  window paints before the roots are read, and every level's count query, so
  the rows are drawn before the counts arrive. Entering a level reads its rows
  synchronously — one indexed `WHERE parent_uid = ?`.
- **Opening a text goes to the last *visible* Sutta Search window**, as a new
  tab, through `emit_show_sutta_from_reference_search("")`. With no such
  window open that route does nothing at all, so `open_node()` checks
  `count_open_sutta_search_windows()` first and opens a window with the result
  instead.
- **`nav_tree_filter_json()` refuses a non-positive limit.** SQLite reads a
  negative `LIMIT` as "no limit", so a slip in the QML would return the whole
  tree as a flat list rather than an error.
- **No data is said, not shown.** When `nav_tree_is_available()` is false the
  window shows "The navigation index is not available." rather than an empty
  list, which would read as a corpus with nothing in it. Nothing populates the
  tables at runtime, so an install whose `appdata.sqlite3` predates the tree
  stays in that state until it downloads a database that carries it.

## 2h. The Timeline page

The **Timeline** button of the window opens a reader tab that shows every node
with a context note on one vertical time axis. It is a page of the localhost
API, `GET /navigation_timeline?<window_id>`, built on the sutta page template,
so it has the app's fonts, themes and find bar. It shows and does not edit; the
one action on it is the button on each entry that opens the window at that node
(below).

### Where the data comes from

`backend/src/navigation_timeline.rs`, with no Qt and no HTTP in it:

1. `timeline_entries_conn()` joins `NavigationTexts::dated_context_notes()` to
   `navigation_nodes` by `node_uid`, in one query, for the titles the window
   shows and the `sort_path`. A note whose key names no node is skipped with a
   warning; the tree build is what reports it.
2. `build_timeline()` groups the entries into **clusters**. A cluster is the
   entries with the same period — identical `from`, `to` **and** `circa` — and
   has one date heading, one marker and one extent bar. Clusters are ordered by
   **sort year** (`from`, else `to`), then by `to` with an absent `to` last,
   then by the tree order of their first entry. Entries with no bound go to a
   separate "Undated" list below the axis.
3. `period_text()` writes the heading from the numbers (`500–200 BCE`,
   `c. 100 BCE – 200 CE`, `after 300 BCE`, `before 100 BCE`).
4. `render_timeline_html()` writes the page content, and
   `html_content::timeline_html_page()` wraps it.

The dates are embedded TOML, so a corrected date needs an app rebuild and
nothing else. With no tree in the database the page shows its controls and "No
timeline data".

### The markup is the contract with the script

The page is complete and readable with no script. `src-ts/timeline.ts` only
spaces the blocks and draws, and reads everything it needs from the markup:

``` html
<div id="timeline" class="timeline">
  <div class="timeline-controls">
    <div class="timeline-toggles">
      <label><input type="checkbox" id="timeline_scale_toggle"> Scale spacing with time</label>
      <label><input type="checkbox" id="timeline_minimap_toggle" checked> Minimap</label>
    </div>
    <p class="timeline-legend">
      <span class="timeline-legend-item" data-form="dated">filled: dated range</span> · …
    </p>
  </div>
  <div class="timeline-body">
    <svg class="timeline-axis" aria-hidden="true"></svg>
    <div class="timeline-clusters">
      <section class="timeline-cluster" data-from="-500" data-to="-200"
               data-circa="false" data-sort-year="-500">
        <h2 class="timeline-period">500–200 BCE</h2>
        <article class="timeline-entry" data-node-uid="dn">…</article>
      </section>
    </div>
  </div>
  <section class="timeline-undated">…</section>
</div>
```

- **An absent bound is an absent attribute**, never `data-to=""`: the script
  reads a missing attribute as "no bound".
- `data-form` on a legend item names the certainty form (`dated`,
  `approximate`, `open`) whose sample the script draws in front of the text,
  with the same functions that draw the axis.
- All text is HTML. **Nothing the script adds is a text node**: the find bar
  (`src-ts/find.ts`) wraps every text node it matches inside `#ssp_content` in
  a `<span>`, which inside an SVG `<text>` would erase the text. The year
  labels of the ticks are therefore empty `<span data-label="500 BCE">`
  elements shown through `content: attr(data-label)`.
- **The page stylesheet applies to everything the script adds**: `suttas.css`
  has `#ssp_main * { max-width: 100% }`, whose id selector outranks any class
  rule in `timeline.scss`. An absolutely positioned element in a container with
  no width is therefore zero wide — which is why the tick-label layer is given
  the gutter's width. Check a new element against the real page, not against
  `timeline.css` alone.

### The blocks stay in document flow

The script sets `margin-top` on each `.timeline-cluster` and positions nothing
but the drawing, which fills the gutter left of the text
(`--timeline-gutter`, set by the script on `#timeline`). Text wrapping at phone
width and the find bar's scroll-into-view therefore need no code.

- **Even spacing** (the default): a fixed gap between clusters.
  **Scaled**: the distance between two markers is
  `max(previous block height + gap, px_per_year × years between the sort years)`.
  It is "scaled", not a linear axis: a block is never squeezed, so the scale is
  stretched across a tall cluster.
- **One piecewise-linear scale for both modes**, from each cluster's sort year
  to its marker's y, built *after* the blocks are placed. Extent bars and year
  ticks are drawn through it.
- **Clusters that share a sort year contribute the last one's y, and a bar
  starts at its own marker.** The scale can hold one y per year; with the last
  one, `scale(to) >= scale(from) >= own marker y` for every cluster. With the
  first one a bar can point backwards.
- A bar with one bound has no second year to reach: it has a fixed length,
  downward for `from` only and upward for `to` only.
- Bars that overlap vertically are put in **lanes**. The gutter is as wide as
  the lanes need, capped at 30% of the page width; beyond the cap the lanes
  share the room.
- **Re-layout happens on a width change only.** The layout changes the height
  of the element the `ResizeObserver` watches, so `layout()` keeps the width it
  last ran for and returns early on the same one. It also runs once when the
  web fonts have loaded, and on the toggle, which keeps the cluster at the top
  of the viewport in place. The toggle state is not kept.

The arithmetic is in `src-ts/timeline_layout.ts`, which is tested with jest and
**must never import d3**: the d3 packages are ESM-only and the jest setup cannot
load them. Only `timeline.ts` imports d3, and it has no test.

### From an entry to the window

Each entry has a button, added by the script, that opens or raises the Sutta
Navigation window **with that node's level entered**:

```
button → GET /open_sutta_navigation?node_uid=…        (bridges/src/api.rs)
       → callback_open_sutta_navigation_at()            (cpp/gui.cpp)
       → signal_open_sutta_navigation_at                (to the GUI thread)
       → WindowManager::open_sutta_navigation_at()      create or revive
       → enter_node_by_uid(node_uid)                    (SuttaNavigationWindow.qml)
```

- **The request arrives on a Rocket worker thread and may have to create the
  window**, so it crosses to the GUI thread through a `WindowManager` signal.
  Calling `create_sutta_navigation_window()` from the callback directly would
  build QML off the GUI thread.
- The route answers 404 for a uid the tree does not have, and makes sure the
  embedded texts are parsed before it hands over: the window's first query is a
  synchronous call on the GUI thread.
- `enter_node_by_uid()` clears the filter and enters the level the way a filter
  hit does (`enter_level()`). A node with no children has no level: its parent's
  is entered.
- **A window created by this request must not load the top level afterwards.**
  `Component.onCompleted` defers that load with `Qt.callLater`, which runs after
  `enter_node_by_uid()`; it is skipped when the breadcrumbs are no longer empty.
- This is the one place that moves the kept window from where the user left it
  ([window-lifecycle-and-reuse.md](./window-lifecycle-and-reuse.md) §5d), because
  that is what the button asks for. A plain open still resets nothing.
- The button is an icon with a `title`, and no text node: a label would be a
  find-bar match in every entry. It is styled with the `--btn-*` custom
  properties of `_menu.scss`, like the page's menu and find buttons, which is
  also what themes it.

### The minimap

A fixed strip under the page's corner buttons shows the whole page: each
cluster as a block in its colour, as tall as its texts are on the page, beside
its extent bar in miniature, with a band over the part in the viewport. A click
or a drag on it scrolls there. The **Minimap** checkbox hides it; like the
spacing toggle it is not kept, and the page always opens with the minimap.

- **Page y maps linearly to strip y** — over the whole document height, not
  over the years. The band and a click's scroll target are then the same
  arithmetic in both directions (`minimap_view()`, `minimap_scroll_target()` in
  `timeline_layout.ts`), in either spacing mode.
- It is redrawn at the end of every `layout()`, from the same measurements as
  the axis, and on a window `resize`: a new viewport height changes the strip
  and the band with no change of width to cause a layout.
- While it is shown, `#timeline` has the class `timeline-minimap-on`, which
  keeps a right margin for it. That changes the text width, so the toggle
  re-lays out.
- It is a child of `<body>` with `background-color: inherit`: the strip lies
  over the page and has to cover it, the stylesheet knows no theme colour, and
  a theme is a class on the body that changes while the page is open — a
  colour read once by the script would stay behind.
- It has no text node, so the find bar does not see it.

### The controls sit under the template's message strip

`page.html` has an empty, absolutely positioned `.transient-messages` strip,
two lines tall, over the top of the content. It is painted above ordinary
content and takes the clicks there. Sutta pages have only a title under it; the
timeline has its checkboxes, which is why `.timeline-controls` is
`position: relative` — positioned and later in the document, it is painted
above the strip. Any other control near the top of a page on this template
needs the same.

### Certainty and colour

| Period | Marker | Extent bar |
|---|---|---|
| both bounds | filled | solid |
| both bounds, `circa` | hollow | dashed |
| one bound | hollow | dashed, fading out, with an arrowhead at the open end |

Certainty is carried by fill, dash and arrow, never by hue. **Colour says only
which cluster a line belongs to**: `timeline.scss` gives each cluster a
`--timeline-color` from a list of eight (`nth-child`), used for the top border
and the tint of its entry cards; the script reads it from the cluster element
and sets `color` on that cluster's bar, marker, fade gradient and arrowhead.
Everything in the drawing is `currentColor`, so the axis line, the ticks and the
legend samples follow the theme's text colour, and the stylesheet has no
`body.dark` rule. The lines are clipped away inside every marker disc, so a
hollow marker stays hollow where another cluster's bar passes behind it.

### A separate bundle

`webpack.config.js` has two entries. d3 (`d3-scale`, `d3-selection`) is only in
`assets/js/timeline.min.js`, which `timeline_html_page()` loads with a
`<script src>` at the end of the page content; `simsapa.min.js`, which every
sutta page loads, does not contain it. Both bundles are committed and served
from the embedded `assets/` tree, so a script change needs `npx webpack` and
then an app rebuild.

### The reader tab

The tab is an ordinary reader tab whose result data is
`{ item_uid: "Timeline", table_name: "navigation_timeline" }`. The `item_uid`
is the tab's label; the `table_name` is what makes the views load the route.
Code that treats "not a dictionary or book type" as a sutta must know the value:

| Where | What it does for the timeline |
|---|---|
| `SuttaNavigationWindow.qml` `open_timeline()` | opens the tab the way `open_node()` opens a leaf |
| `SuttaHtmlView_Desktop.qml`, `SuttaHtmlView_Mobile.qml` | `load_timeline()` in both dispatch blocks; `is_sutta_content()` is false |
| `SuttaSearchWindow.qml` `show_result_in_html_view()` | focuses an existing timeline tab instead of opening a second; loads no translations |
| `SuttaSearchWindow.qml` `open_bookmark_in_tab_group()` | the same one-tab rule for a restored session or a bookmark |

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

**An SC spine build also rewrites `assets/navigation-blurb-suttacentral.toml`**
(§2f), in the full bootstrap and in `build-navigation-tree` alike, with no extra
flag. The folder is `../assets` from the working directory — right when the CLI
is run from `cli/` — or the one given with `--assets-dir`. When the file
changed, **rebuild the app**: the summaries are embedded in it. Both spine
builds also check the keys of the two hand-written texts files and report a
bad one in their defect summary.

`build-navigation-tree` does not drop columns: a database bootstrapped while
the table still carried `blurb` and `context_note` keeps both, unread, until a
full re-bootstrap.

## 7. Related

- [database-migrations.md](./database-migrations.md) — the migrations that
  create `navigation_nodes` and add its later columns, and why a migration
  failure is non-fatal.
- [user-data-and-sqlite-analyze.md](./user-data-and-sqlite-analyze.md) — the
  `ANALYZE` that follows each spine write.
- [window-lifecycle-and-reuse.md](./window-lifecycle-and-reuse.md) §5d — why
  the window is kept across a close, and why its close must be accepted.
- `2026-09-18-174900-prd---sutta-navigation-window.md`, under `tasks/` or
  `tasks/archive/` — the PRD, whose FR-15 carries a dated correction about
  `VERIFIED_CST_FILES`.
