# PRD — Navigation summaries and context notes as app-embedded TOML

**File:** `tasks/2026-10-03-054059-prd---navigation-summaries-as-embedded-toml.md`
**Status:** Draft for implementation
**Feature name:** `navigation-summaries-as-embedded-toml`

**Context — read first:**

- The original feature:
  [`tasks/archive/2026-09-18-174900-prd---sutta-navigation-window.md`](./archive/2026-09-18-174900-prd---sutta-navigation-window.md)
  and its task list
  [`tasks/archive/2026-09-18-174900-tasks-sutta-navigation-window.md`](./archive/2026-09-18-174900-tasks-sutta-navigation-window.md).
  This PRD **supersedes** that PRD's storage of blurbs (the `blurb` column in
  its FR-9 table) and nothing else. Its row-rendering and toggle requirements
  (FR-31, FR-33, FR-43 to FR-46, as corrected) stand unchanged.
- The current behaviour: [`docs/sutta-navigation-tree.md`](../docs/sutta-navigation-tree.md)
  §2e (the Summaries toggle) and §2f (context notes).

---

## 1. Introduction / Overview

A row in the Sutta Navigation window can show two pieces of prose under its
title: a **summary** (SuttaCentral's *blurb*) and a **context note** (the
project's own text). Today both are written into `appdata.sqlite3` by the CLI
bootstrap, in the `navigation_nodes.blurb` and `navigation_nodes.context_note`
columns.

That is the wrong place for them. `appdata.sqlite3` is downloaded once at
first-run setup and then kept across app updates, so a corrected summary or a
new context note reaches a user only when they download an entire new database.

This feature moves both texts **out of the database and into the app itself**:
TOML files under `assets/`, embedded in the binary as static strings (the
pattern of `CIPS_GENERAL_INDEX_JSON` in `backend/src/app_settings.rs`), and
deserialized when the Sutta Navigation window first needs them.

**Goal:** a correction to a summary or a context note ships with an ordinary app
update, and all the text involved is in hand-reviewable TOML files in the
repository.

---

## 2. Goals

1. Summaries and context notes update with an **app update**, with no database
   download and no database version bump.
2. All three text sources are **TOML files in `assets/`**, readable and
   editable by hand, and reviewable as a `git diff`.
3. The project can **correct, add or suppress** a SuttaCentral summary without
   editing SuttaCentral's data and without its correction being lost the next
   time the summaries are regenerated.
4. The SuttaCentral summaries file is regenerated **automatically by the CLI
   bootstrap** — nobody has to remember a separate command when
   re-bootstrapping.
5. The Sutta Navigation window looks and behaves **exactly as it does today**.
6. A mistyped key in a hand-written file is still **reported**, as it is today.

---

## 3. User Stories

- **As a reader**, I want a corrected summary to appear after I update the app,
  so that I do not have to re-download several hundred megabytes of databases
  for a text fix.
- **As the maintainer**, I want to fix a wrong or unhelpful SuttaCentral summary
  by adding one entry to a small overrides file, so that my fix survives every
  later regeneration from SuttaCentral's data.
- **As the maintainer**, I want to give a summary to a node SuttaCentral has
  none for, or hide one that is misleading in Simsapa's tree, using the same
  file.
- **As the maintainer**, I want the SuttaCentral summaries file to be rewritten
  whenever I re-bootstrap, so that refreshing the `sc-data` checkout is all it
  takes to pick up SuttaCentral's own changes.
- **As the maintainer**, I want a typo in a key to show up in the bootstrap's
  defect report, so that a note that lands on no node is not silently lost.

---

## 4. Concepts and Terminology

| Term | Meaning |
|---|---|
| **Summary** / **blurb** | The same thing. "Summaries" is the UI label; `blurb` is SuttaCentral's term and the name used in data and code. |
| **Context note** | The project's own note for a node, drawn below the summary. |
| **`node_uid`** | The navigation tree's stable id for a node (`dn`, `sn56.11`, `sutta/linked`, `cst:tika`). It is **not** the `suttas.uid` (`sn56.11/pli/ms`). |
| **SC source uid** | The key SuttaCentral's `blurbs.json` uses. Equal to the `node_uid` for everything from a `*-tree.json`; different for super-tree grouping levels, whose `node_uid` is qualified by ancestors (`sutta/linked` for the source uid `linked`). |
| **Generated file** | Written by the CLI, never edited by hand. |
| **Hand-written file** | Edited by the maintainer, never written by the CLI. |

### The three files

| File | Kind | Content |
|---|---|---|
| `assets/navigation-blurb-suttacentral.toml` | **Generated** | SuttaCentral's English blurbs, regenerated wholesale from `sc-data/additional-info/blurbs.json`. |
| `assets/navigation-blurb-overrides.toml` | **Hand-written** | The project's corrections to the above. Wins at load time. |
| `assets/navigation-context-notes.toml` | **Hand-written** | Context notes. Already exists; content and format unchanged. |

All three use **one format and one key scheme**: a flat TOML table of
`"<node_uid>" = """<text>"""`.

---

## 5. Functional Requirements

### 5.1 File format (all three files)

**FR-1.** Each file is a flat TOML table. Every key is a quoted string, every
value is a string. Any other shape (a nested table, a non-string value, an
unquoted dotted key such as `sn56.11 = "…"`) is a parse failure for the whole
file.

**FR-2.** The value text follows the existing context-note wrapping rule
(`unwrap_note()` in `cli/src/bootstrap/navigation_tree/mod.rs`): line breaks
inside a value are joined with a single space, runs of whitespace collapse, and
a blank line starts a new paragraph. The text the app displays is the
**unwrapped** text.

> **Measured consequence, accepted.** The database held each blurb trimmed but
> otherwise verbatim; the unwrapped form also collapses runs of spaces. Over
> the shipped data that changes **13 of 4,111** blurbs, each by one double
> space becoming a single space (`mn7`, `mn87`, `sn22.2`, `sn35.133`,
> `sn35.158`, `sn35.159`, `sn35.245`, `sn36.5`, …). No blurb contains a line
> break, a tab, a no-break space or a control character, and no context note
> is changed at all. This is the one permitted difference under FR-35.

**FR-3.** There must be **one** parser for this format, used for all three
files, living in `backend` (it is needed at runtime). The CLI must call the
backend's parser; the copy currently in the CLI (`parse_context_notes()` /
`unwrap_note()`) must be removed, not kept alongside.

**FR-4.** The parser must be able to return entries whose text is **empty**,
because an empty override is meaningful (FR-13). For the SuttaCentral file and
the context-notes file an empty entry means "nothing to show" and is dropped, as
today.

### 5.2 The generated file — `navigation-blurb-suttacentral.toml`

**FR-5.** The file is produced from the **English** section of
`sc-data/additional-info/blurbs.json` (`{ "en": { "<grouping>": { "<uid>":
"<blurb>" } } }`). No other language is read.

**FR-6.** The file contains **one entry per tree node that has a blurb**, keyed
by that node's `node_uid` — and nothing else. Every key in the file is therefore
the uid of a node in the tree. A `blurbs.json` entry that no node uses is **not
written**.

**"Used by a tree node" means a node in the final tree**, after the SC builder
has collapsed its single-member grouping levels. A level that was built and then
elided (`long`, `linked`, …) is not in the tree.

> **Measured over the shipped database and `blurbs.json`** (4,189 English
> entries): 4,111 nodes carry a blurb. For 4,110 of them the `node_uid` equals
> the SuttaCentral uid, with identical text; the single exception is
> `sutta/minor/kn`, whose source uid is `kn`. The other 78 entries belong to no
> node: 7 are re-keyed by FR-7a, and 71 are skipped by FR-7. The generated file
> therefore has **4,120 entries** (4,111 plus the nine FR-7a targets).

**FR-7. The entries that are skipped are a known list, written down in the
code with the reason for each group.** The exporter does not skip "whatever no
node uses": it carries an explicit skip list, and every `blurbs.json` entry must
be accounted for as exactly one of *used by a node*, *re-keyed* (FR-7a) or
*on the skip list*.

| Group | Count | Why it is skipped |
|---|---|---|
| Grouping words | 6 | `long`, `medium`, `linked`, `numbered`, `minor`, `other` — super-tree levels that hold one member in the Pāli-only scope and are elided from the tree, or are never built. Their text is the cross-tradition summary; the surviving node carries the specific one. |
| Non-Theravāda collections | 44 | `da`, `da-ot`, `ma`, `ma-ot`, `sa`, `sa-2`, `sa-3`, `sa-ot`, `ea`, `ea-2`, `ea-ot`, `lzh-ja`, `lzh-dhp`, `lzh-nbs`, `lzh-ssnp`, `lzh-dharani`, `lzh-svk`, `arv`, `avs`, `divy`, `lal`, `uv`, `mkv`, `sht`, `sf`, `d`, `up`, `pdhp`, `pf`, `gdhp`, `gf`, `kf`, `uf`, `sg`, `lzh-dk`, `pr`, `dy`, `vk`, `pk`, `jp`, `jp-2`, `mvs`, `sab`, `san-dk` — outside the tree's Theravāda Pāli scope; Simsapa has no texts for them. |
| Per-rule Vibhaṅga blurbs | 21 | `pli-tv-bu-vb-as1`…`as7`, `pli-tv-bi-vb-as1`…`as7`, `pli-tv-bi-vb-pj1`…`pj4`, `pli-tv-bi-vb-pc91`…`pc93` — the tree holds each passage as one ranged leaf (`…-as1-7`, `…-pj1-4`, `…-pc91-93`), so there is no per-rule node; and each text is byte-identical to the Pātimokkha blurb (`pli-tv-bu-pm-as1`, …) the tree already shows. |

The reason for each group must be stated in a comment beside the list in the
code, so that the next person to see an unexpected entry knows what the list is
for and which group a new key belongs in.

Because the list is explicit, the exporter can and must report what it does not
recognise, as defects, without failing the build:

1. **An entry no node uses that is neither re-keyed nor on the skip list** —
   a new upstream entry, or a new mis-keying. It is reported and not written;
   the maintainer then adds it to the skip list or to FR-7a's table.
2. **A skip-list key that is no longer in `blurbs.json`**, or that a node now
   uses (in which case the node gets its blurb as usual) — a stale row.
3. **A per-rule Vibhaṅga entry whose text no longer equals its Pātimokkha
   counterpart** — the "redundant" reason no longer holds for it.

**FR-7a. Seven known mis-keyed blurbs are re-keyed by an explicit table.**
`blurbs.json` files seven Theravāda blurbs under a uid its own tree does not
use (Open Question 1 has the analysis), so nine nodes show no summary. The
exporter carries a **hand-maintained table** of exactly these cases and writes
each blurb under the `node_uid` it belongs to:

| `blurbs.json` key | Written under | Prefix removed from the text |
|---|---|---|
| `sn17.37-43` | `sn17.37` **and** `sn17.38-43` | — |
| `sn33.51-55` | `sn33.51-54` **and** `sn33.55` | — |
| `sn39.1` | `sn39.1-15` | — |
| `an3.156` | `an3.156-162` | — |
| `sn23.159` | `sn24.1` | `sn24.1: ` |
| `sn28.159` | `sn29.1` | `sn29.1: ` |
| `pli-tv-bu-pm-an` | `pli-tv-bu-pm-ay` | — |

Rules for the table:

1. **It is a list of named cases, not a heuristic.** The exporter must not
   infer a re-keying — no range arithmetic, no fuzzy matching, no "nearest
   uid". A mis-keyed blurb that is not in the table is reported under FR-7
   and not written.
2. A re-keyed entry is a tree entry (FR-9), and its comment line also names the
   source key, e.g. `# Vātasutta (blurbs.json key: sn23.159)`, so the file
   shows that the key was adapted.
3. Where a blurb spans a range the tree splits in two, both nodes get the same
   text.
4. **Every row is checked on every run, and a row that no longer applies is
   reported as a defect and skipped**, never forced:
   - the source key is no longer in `blurbs.json`;
   - a target `node_uid` is not in the tree;
   - a target node now has a blurb under its own uid — SuttaCentral fixed it,
     and the source's own entry wins;
   - the stated prefix is not at the start of the text.

   This is what tells the maintainer to delete a row once the upstream data is
   corrected, and what keeps a stale row from overwriting good data.
5. The 21 redundant per-rule Vibhaṅga blurbs are **not** in the table: their
   text is already shown on the Pātimokkha rule nodes.

The defects remain SuttaCentral's and are still worth reporting upstream; the
table is the project's workaround until they are fixed, and the only place the
exporter departs from the key the source gives.

**FR-8.** The blurb text is written **as SuttaCentral wrote it**, apart from
trimming leading and trailing whitespace (today's `clean()`) and the two
prefixes FR-7a names. The exporter must
not rephrase, re-punctuate or strip markup. Whatever characters TOML requires to
be escaped inside the chosen string form (`\`, a run of three quotes) must be
escaped so the file always parses.

**FR-9.** The output must be **deterministic**: the same `blurbs.json` and the
same tree produce a byte-identical file, so that a re-bootstrap with unchanged
sources leaves `git status` clean. Specifically:

1. Entries are in tree order (the order the window shows them).
2. The skip list and the re-keying table are fixed data in the code, not
   derived from the run.
3. Each entry is preceded by a `#` comment line giving the node's
   displayed title, as `navigation-context-notes.toml` does, so a reviewer can
   tell what `"an4.159"` is without looking it up.
4. Long text is wrapped across lines at a fixed width, for readable diffs.
5. No timestamp, no machine path, no run-specific content anywhere in the file.

**FR-10.** The file begins with a comment header stating that it is
**generated**, which command generates it, that it must **not be edited by
hand**, and that corrections belong in `navigation-blurb-overrides.toml`.

**FR-11. Round-trip.** Parsing the written file with the FR-3 parser must
yield, for every entry, exactly the text that went in (after FR-2's whitespace
normalization). This must be covered by an automated test over synthetic input
that includes a `\`, quote characters, and text long enough to be wrapped.

### 5.3 The overrides file — `navigation-blurb-overrides.toml`

**FR-12.** The file is created by this feature with an explanatory comment
header and **no entries**. The header documents the three things an entry can do
(FR-13), the always-quote-the-key rule, the wrapping rule, and how to find a
`node_uid`.

**FR-13.** For a given `node_uid`, an override does one of three things:

| Override | SuttaCentral file has an entry | Result shown |
|---|---|---|
| non-empty text | yes | the override text (**replace**) |
| non-empty text | no | the override text (**add**) — this works for any node, including CST nodes |
| empty string `""` | yes | no summary (**suppress**) |
| empty string `""` | no | no summary; reported as a defect by FR-23, since it suppresses nothing |

**FR-14.** The CLI must never write to this file.

### 5.4 The context-notes file

**FR-15.** `assets/navigation-context-notes.toml` keeps its name, location,
format, keys and content. Only its comment header changes: the line telling the
maintainer to "rebuild the CLI and re-run `build-navigation-tree`" is replaced
by the new procedure (rebuild the app).

### 5.5 Embedding and loading in the app

**FR-16.** The three files are embedded in the `backend` crate as `static`
strings with `include_str!`, next to `CIPS_GENERAL_INDEX_JSON` in
`backend/src/app_settings.rs` (or in `backend/src/navigation_tree.rs` if that
reads better — but all three together, in one place).

**FR-17.** The files are **not** parsed at startup. They are parsed **once**,
the first time a navigation query needs them — in practice when the Sutta
Navigation window is first opened — and the parsed maps are kept for the life of
the process.

**FR-18.** Nothing may be added to the startup path. In particular, no parse and
no warm-up from `AppData::new()`, `init_app_data()` or any
`Component.onCompleted` of `SuttaSearchWindow` (see
[docs/startup-sequence-and-caches.md](../docs/startup-sequence-and-caches.md)).

**FR-19.** The summary and the context note are attached to a node **in Rust**,
where the node's JSON view is built (`NavigationNodeView` in
`backend/src/navigation_tree.rs`), for all three query routes: a level's
children, filter hits, and a single node. The JSON the bridge returns keeps the
**same field names** (`blurb`, `context_note`) it has today.

**FR-20.** The summary for a node is resolved as:

1. If the overrides file has an entry for the `node_uid`: use it; if it is
   empty, the node has no summary.
2. Otherwise, the SuttaCentral file's entry for the `node_uid`, if any.
3. Otherwise none.

The context note is the context-notes file's entry for the `node_uid`, if any.

**FR-21. A file that fails to parse must not break the window.** The failure is
logged once as an error naming the file, that file is treated as empty, and the
other two are still used. The tree itself — titles, navigation, opening texts —
must work regardless. (FR-22 is what stops a broken file from being shipped in
the first place.)

**FR-22.** An automated test in `backend` must parse all three **embedded**
files with the real parser and fail if any of them does not parse. This is the
guard that makes FR-21's fallback a last resort rather than a normal path.

**FR-22a. The parse is started off the GUI thread when the window is
requested.** The window's first query runs synchronously on the GUI thread
(`SuttaNavigationWindow.qml` calls `nav_tree_children_json()` from its initial
load), so a purely lazy parse would stall that thread for the whole parse time.
`SuttaBridge::open_sutta_navigation_window()` must therefore start a background
thread that initializes the cache before it hands over to the window manager.
Loading the window's QML engine takes longer than the parse is expected to, so
the cache is normally ready before the first query; if it is not, that query
waits for the same initialization rather than starting a second one. This is
not a startup cost (FR-18): it runs only when the user asks for the window.

### 5.6 Key validation at bootstrap

**FR-23.** The navigation tree build (`build_sc_spine` / `build_cst_spine`,
reached from both the full bootstrap and `build-navigation-tree`) must validate
the keys of the two **hand-written** files against the tree it has just built,
and record in the existing defect report:

| Defect | File |
|---|---|
| a key that names no node in the tree | context notes, overrides |
| an empty override for a node that has no SuttaCentral blurb (it suppresses nothing) | overrides |

It writes nothing from these files into the database.

**FR-24.** The two spines are built independently, so the existing ownership
rule is kept: a `cst:`-prefixed key is checked by the CST spine, every other key
by the SC spine. A key is reported by the spine that owns it and ignored by the
other.

**FR-25.** Keys of the **generated** file need no validation against the tree:
by FR-6 every one is the uid of a node in the tree that was just built.

**FR-26.** Defects are **reported, never repaired** (the standing source-data
rule): the build does not drop, rename or guess a key, and does not fail because
of one.

**FR-26a.** A hand-written file that does not **parse at all** still fails the
tree build, as the context-notes file does today. FR-21's tolerance belongs to
the app at runtime; at bootstrap there is a person to fix the file, and
continuing would hide every key in it from FR-23.

### 5.7 Regeneration as part of the bootstrap

**FR-27.** The full CLI bootstrap must regenerate
`assets/navigation-blurb-suttacentral.toml` in the **repository's** `assets/`
folder as part of building the SC spine, with no extra command and no extra
flag.

**FR-28.** The standalone `build-navigation-tree` command must regenerate it
too, whenever it builds the SC spine.

**FR-29.** The write must be all-or-nothing, and a re-bootstrap must never
replace a good file with an empty or partial one. The file is written only
after `blurbs.json` has loaded with at least one English entry **and** the SC
spine has been built and written to the database; it is written to a temporary
file and renamed into place. In every other case the existing file is **left
untouched** and a warning says why. How the tree build itself reacts to a
missing `blurbs.json` (today it fails) is not changed by this feature.

**FR-30.** After writing, the bootstrap logs the path, the number of entries
written, the number skipped and re-keyed, and a clear reminder that
**the app must be rebuilt** for the change to take effect — the running CLI and
any existing app binary still embed the previous file.

### 5.8 Database and bootstrap clean-up

**FR-31.** The `blurb` column is removed from the
`2026-09-19-000000_navigation_tree` migration's `up.sql`, and the
`2026-10-01-000000_navigation_node_context_note` migration folder is **deleted
outright** (not moved to an archive folder — `embed_migrations!` walks the
tree).

> **This edits migrations in place, which is normally forbidden.** It is
> allowed here only because both migrations are newer than the last release tag
> (`v1.0.0-alpha.8`, 2026-09-18) and have reached no user. Confirm that is still
> true before starting; if a build containing them has been released, stop and
> raise it — the answer then is a new drop-column migration instead.

**FR-32.** The `blurb` and `context_note` fields are removed from the Diesel
schema (`backend/src/db/appdata_schema.rs`) and from the database-row structs
(`NavigationNode`, `NewNavigationNode`). They remain on the JSON view struct,
filled per FR-19.

**FR-33.** The bootstrap no longer writes either text to the database:
`apply_context_notes()`, the `blurb` field of the CLI's tree node, and the
CLI's own `include_str!` of the context-notes file are removed. `blurbs.json` is
read only to produce the generated file.

**FR-34.** Every read of the navigation tables must keep tolerating a database
whose `navigation_nodes` table is absent, empty, or — on a developer machine —
still carries the two old columns.

### 5.9 Unchanged behaviour

**FR-35.** The following must be identical before and after, for a database and
files with the same content:

- which rows show a summary and which a context note, and their text (with the
  two measured exceptions: 13 blurbs lose a double space (FR-2), and nine nodes
  that showed no summary gain one (FR-7a));
- the Summaries toggle, its persisted setting (`show_navigation_blurbs`) and its
  bridge functions;
- two-line truncation and the separate more/less buttons;
- the filter: it matches Pāli title, English title and uid, and **not** the
  summary or the note.

**FR-36.** No bridge function is added, removed or renamed. (If the
implementation finds it must change the bridge API, build through the `Makefile`
so `scripts/qmltypes-guard.sh` runs — see
[docs/qmltypes-aot-staleness.md](../docs/qmltypes-aot-staleness.md).)

### 5.10 Documentation

**FR-37.** Update, describing current behaviour only:

- `docs/sutta-navigation-tree.md` §2e and §2f (sources, the three files, the
  override rules, the edit-and-rebuild procedure, where key defects are
  reported) and its rebuild-commands section;
- the `docs/sutta-navigation-tree.md` entry in `CLAUDE.md`;
- `PROJECT_MAP.md`.

---

## 6. Non-Goals (Out of Scope)

1. **No in-app update or download** of summaries or notes (nothing like the
   Topic Index's Update / Reset). They change with app releases only.
2. **No change to the navigation tree's structure, titles or node uids**, which
   stay in the database and are still built at bootstrap.
3. **No other languages.** English blurbs only, as today.
4. **No UI change** of any kind.
5. **No editing of SuttaCentral's text** in the generated file or in `sc-data`,
   beyond the two key prefixes FR-7a removes;
   corrections go in the overrides file.
6. **No database version bump and no app version bump** as part of this work —
   the maintainer does those.
7. **No migration for existing users.** The columns being removed never
   shipped.
8. **No new content.** Writing overrides or further context notes is separate
   work.

---

## 7. Design Considerations

There is no visual design work. The one design choice is the shape of the
generated file, which is optimised for **review as a diff**: tree order, a title
comment per entry, wrapped lines, and a stable byte-for-byte output (FR-9).
`assets/navigation-context-notes.toml` is the model for how an entry should
look.

---

## 8. Technical Considerations

**T-1. The `toml` crate is a dependency of `cli` only.** `backend` needs it for
the runtime parser (FR-3). Add it to `backend` at the latest release,
`toml = "1.1.6"`, and raise the CLI's `toml = "0.8"` to the same version in the
same change, so the two crates do not carry two copies of the parser. The CLI
upgrade crosses a major version: check every existing `toml` call site in
`cli/` still compiles and that its tests pass. `toml` 1.1.6 requires Rust 1.85;
the local toolchain is 1.96, and the toolchains used for the other platforms'
release builds must meet it too.

**T-2. Size and parse cost.** The generated file is expected to be about
0.9 MB: roughly 0.7 MB of blurb text plus keys, quoting and the per-entry
title comments. That is smaller than `general-index.json` (2.3 MB), but TOML
parses far more slowly than JSON, so the size comparison understates the cost.
The estimate — not a measurement — is tens of milliseconds on a desktop release
build and up to a few hundred on a low-end phone, once per process. It happens
on first use of the window, so **measure the parse time** in a release build on
desktop and on a phone (a debug build is several times slower and is not the
number that matters) and record it in the commit message. If it is long enough
to be felt when the window opens, raise it rather than moving the parse to
startup (FR-18); see Open Question 2 for the options.

**T-3. Thread safety of the lazy cache.** Navigation queries can be called from
more than one thread. A `OnceLock` (or `LazyLock`) holding the parsed maps is
sufficient; no lock is needed after initialization because the data never
changes at runtime.

**T-4. Where the repository's `assets/` folder is.** The bootstrap already
derives its input folder from the current working directory (it is run from
`cli/`, and resolves `../../bootstrap-assets-resources`; see
`cli/src/bootstrap/mod.rs`). The output path for FR-27 should be derived the
same way (`../assets/` from `cli/`), and `build-navigation-tree` should accept
an optional argument to override it. If the resolved folder does not exist, log
a warning and skip the write — do not create an `assets/` folder somewhere
unexpected.

**T-5. The generated file must exist in the repository.** `include_str!` fails
the build if the file is missing, and the CLI depends on `backend`, so the CLI
that generates the file cannot be built without it. The file is therefore
**committed**, and produced once as part of this work.

**T-6. The embedded copy is always one build behind a regeneration.** A
bootstrap rewrites the file on disk; the CLI running it, and any app binary,
still embed the old one. Key validation (FR-23) reads only the hand-written
files, which the bootstrap does not change, so it is unaffected. FR-30's
reminder covers the rest.

**T-7. Developer databases.** A local `appdata.sqlite3` that already applied the
two migrations keeps the old columns and a ledger row for a migration that no
longer exists. Diesel applies only migrations missing from the ledger, so this
is harmless, and a re-bootstrap clears it. The database used for release must be
re-bootstrapped after this change.

**T-8. Never commit a database.** The bootstrap run needed to produce the
generated file also rewrites `appdata.sqlite3`. Check `git status` before
committing.

**T-9. Markup in blurbs.** 112 SuttaCentral blurbs contain `<`. They are
displayed today exactly as stored, and FR-8 keeps that. Do not add or remove any
markup handling.

**T-10. Tests to move.** The CLI's existing context-note tests
(`cli/src/bootstrap/navigation_tree/tests.rs`) cover parsing and key reporting;
the parsing ones move to `backend` with the parser, the key-reporting ones stay
with the validation.

---

## 9. Success Metrics

1. **Same output.** For the current data, the set of `(node_uid, summary,
   context note)` the window receives is identical before and after the change,
   apart from the two exceptions named in FR-35: 13 summaries differ by one
   space, and exactly nine nodes gain a summary (the FR-7a targets). Verify by
   dumping both from the current database (4,111 summaries, 44 notes) before
   starting and comparing against the new lookup (4,120 summaries, 44 notes).
2. **Correction without a database.** Adding an entry to
   `navigation-blurb-overrides.toml` and rebuilding the app — with no bootstrap
   run and no change to `appdata.sqlite3` — changes what the window shows.
   Verified once each for replace, add (on a CST node) and suppress.
3. **Clean regeneration.** Running the navigation tree build twice in a row
   leaves `git status` clean after the second run.
4. **Defects still reported.** A deliberately misspelled key in each
   hand-written file appears in the defect report of the spine that owns it.
5. **Smaller database.** `navigation_nodes` has no `blurb` or `context_note`
   column in a freshly bootstrapped database.
6. `cd backend && cargo test`, the CLI tests and `make qml-test` pass, and
   `make build -B` succeeds.

---

## 10. Open Questions

1. **Reporting the seven mis-keyed blurbs to SuttaCentral.** The 78
   `blurbs.json` entries no node uses were examined against `blurbs.json`, the
   `*-tree.json` files and the built tree, and are all accounted for: 71 are
   skipped (FR-7, with the reason per group) and 7 are re-keyed (FR-7a). The
   seven are defects in SuttaCentral's data:

   | `blurbs.json` key | Cause | Node(s) that showed no summary |
   |---|---|---|
   | `sn17.37-43` | different range boundary | `sn17.37`, `sn17.38-43` |
   | `sn33.51-55` | different range boundary | `sn33.51-54`, `sn33.55` |
   | `sn39.1` | single uid vs the tree's range | `sn39.1-15` |
   | `an3.156` | single uid vs the tree's range | `an3.156-162` |
   | `sn23.159` | wrong key — the text begins `sn24.1: …` | `sn24.1` |
   | `sn28.159` | wrong key — the text begins `sn29.1: …` | `sn29.1` |
   | `pli-tv-bu-pm-an` | different abbreviation — the tree's uid is `pli-tv-bu-pm-ay` | `pli-tv-bu-pm-ay` |

   None of these keys appears in any `*-tree.json` or has a row in `suttas`,
   and `blurbs.json` has no entry under any of the uids the tree does use for
   them. Should they be reported upstream? Once fixed there, FR-7a's checks
   flag each row for deletion.
2. **First-open parse time (T-2).** No measurement exists yet. If it is
   noticeable on a phone, the options are, cheapest first: a background parse
   started when the window is created, with the first level drawn without
   summaries until it finishes; or dropping the per-entry title comments and
   line wrapping to shrink the file. (The parser is already the latest `toml`
   release, per T-1.) To be decided on the measurement, not before.

---

## 11. Not to be done

Per the workflow: **this PRD is not to be implemented as part of producing it.**
Generate the task list separately.
