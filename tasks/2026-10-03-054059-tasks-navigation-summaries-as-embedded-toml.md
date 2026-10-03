# Tasks — Navigation summaries and context notes as app-embedded TOML

PRD: [`2026-10-03-054059-prd---navigation-summaries-as-embedded-toml.md`](./2026-10-03-054059-prd---navigation-summaries-as-embedded-toml.md)

Background: [`archive/2026-09-18-174900-prd---sutta-navigation-window.md`](./archive/2026-09-18-174900-prd---sutta-navigation-window.md),
[`docs/sutta-navigation-tree.md`](../docs/sutta-navigation-tree.md) §2e, §2f, §6.

## Relevant Files

**Assets**

- `assets/navigation-blurb-suttacentral.toml` - **New, generated.** SuttaCentral's English blurbs, rewritten by the CLI. Never edited by hand. Committed.
- `assets/navigation-blurb-overrides.toml` - **New, hand-written.** The project's corrections; starts with a header and no entries.
- `assets/navigation-context-notes.toml` - Existing. Content unchanged; only the header's "how to apply an edit" line changes.

**Backend**

- `backend/Cargo.toml` - Add `toml = "1.1.6"`.
- `backend/Cargo.lock`, `cli/Cargo.lock`, `bridges/Cargo.lock` - Updated by the `toml` 1.1.6 change.
- `backend/src/navigation_texts.rs` - **New.** The one parser for the three files (`parse_node_texts`, `unwrap_note`), the `NavigationTexts` struct with the override resolution, and the lazily initialised process-wide cache. Tests alongside (`#[cfg(test)] mod tests` or `navigation_texts/tests.rs`, following `navigation_tree`'s layout).
- `backend/src/lib.rs` - Register `pub mod navigation_texts;`.
- `backend/src/app_settings.rs` - The three `include_str!` statics, next to `CIPS_GENERAL_INDEX_JSON` (line 9).
- `backend/src/navigation_tree.rs` - `NavigationNodeView` filled from `NavigationTexts` instead of row columns; `blurb` / `context_note` removed from `NavigationNode` and `NewNavigationNode`; the comment in `tree_is_available_conn` updated.
- `backend/src/navigation_tree/tests.rs` - Fixtures and tests that set `blurb` / `context_note` on rows; the `include_str!` of the 2026-10-01 migration (line 19).
- `backend/src/db/appdata_schema.rs` - Remove `blurb` and `context_note` from `navigation_nodes` (lines 297-300).
- `backend/migrations/appdata/2026-09-19-000000_navigation_tree/up.sql` - Remove `blurb TEXT,` (line 37).
- `backend/migrations/appdata/2026-10-01-000000_navigation_node_context_note/` - **Delete the folder.**

**CLI**

- `cli/Cargo.toml` - `toml = "0.8"` → `"1.1.6"`.
- `cli/src/bootstrap/navigation_tree/blurb_export.rs` - **New.** `collect()` / `render()` / `write()` for the generated file, the three skip lists and the re-keying table (`BlurbTables`, passed to `collect()`), `default_assets_dir()`. Tests alongside.
- `cli/src/bootstrap/navigation_tree/mod.rs` - `TreeNode` (`blurb` → `source_uid`), `flatten_node`, `build_sc_spine` / `build_cst_spine`, `parse_context_notes` / `unwrap_note` / `CONTEXT_NOTES_TOML` / `apply_context_notes` (removed), `validate_node_text_keys` + `CheckedKeys` (the validate-only pass over both hand-written files), `export_blurbs` (returns the node uids that have a blurb).
- `cli/src/bootstrap/navigation_tree/sc_spine.rs` - `make_node` (line 765), `load_blurbs` (line 1070), the `blurbs` field of the builder.
- `cli/src/bootstrap/navigation_tree/tests.rs` - Context-note tests at lines 209-288: parsing ones move to `backend`, key-reporting ones are rewritten for the validate-only pass.
- `cli/src/bootstrap/mod.rs` - Passes the repository `assets/` path to `build_sc_spine` (line 310).
- `cli/src/main.rs` - `build_navigation_tree()` (line 772) and the `BuildNavigationTree` command (line 1552): new optional `--assets-dir`.
- `cli/src/bootstrap/library_imports.rs`, `cli/src/bootstrap/chanting_practice.rs` - Existing `toml::from_str` call sites; must still compile and pass after the version bump.

**Docs**

- `docs/sutta-navigation-tree.md` - §2e, §2f, §6.
- `CLAUDE.md` - The "Sutta navigation tree" entry.
- `PROJECT_MAP.md` - Lines mentioning `navigation-context-notes.toml`, `navigation_tree.rs`, `bootstrap/navigation_tree/`, and "Navigation tree storage".
- `docs/navigation-context-notes-research.md` - Check for a stale "how to apply" instruction.

**Bridge**

- `bridges/src/sutta_bridge.rs` - `open_sutta_navigation_window()` (line 5948) gains the background warm-up (PRD FR-22a). **Body only** — no `#[qinvokable]` or `#[qsignal]` is added, removed or renamed.

**Not touched:** `bridges/assets/qml/**` and the `SuttaBridge.qml` stub. The bridge API and the JSON field names do not change (PRD FR-36).

### Notes

- Rust tests: `cd backend && cargo test`, `cd cli && cargo test`. Timing-assertion failures in `cargo test` are known drift, not regressions.
- Build the app with `make build -B`, never a bare `cmake --build`.
- **Never commit `appdata.sqlite3`** or any `*.sqlite3`. Several tasks below write to the database; run `git status` before any commit.
- Do not bump the app or database version. Do not run `git commit` without an explicit go-ahead.
- Do not run the GUI. Checks that need the window open are listed in task 5.8 for the user.
- **The SC spine build needs ArangoDB running** (`connect_to_arangodb()`, for the `names` titles). Tasks 2.9, 4.11 and 5.4 run it. If ArangoDB is not reachable, stop and ask the user — do not work around it.
- CLI commands are run from `cli/` (the bootstrap resolves its inputs relative to that directory). The runtime database is
  `/home/gambhiro/prods/apps/simsapa-ng-project/bootstrap-assets-resources/dist/simsapa/app-assets/appdata.sqlite3`.
- Use `try_exists()`, never `.exists()`, in any new path check in `backend`.
- Docs and comments describe current behaviour only — no "this used to be in the database" narration. Measurements go in the commit message.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. This helps track progress and ensures you don't skip any steps.

Example:
- `- [ ] 1.1 Read file` → `- [x] 1.1 Read file` (after completing)

Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

### 1.0 — specs and dependencies

**Depends on:** nothing. **Leaves:** behaviour unchanged; the bootstrap still writes both columns.

**Parser API** (new `backend/src/navigation_texts.rs`):

``` rust
/// Flat table of "<node_uid>" = "<text>". Values are unwrapped (FR-2).
/// Entries whose text is empty are KEPT — the caller decides (FR-4).
pub fn parse_node_texts(toml_text: &str) -> anyhow::Result<BTreeMap<String, String>>;

/// Lines joined with one space, whitespace runs collapsed, blank line = "\n\n".
pub fn unwrap_note(note: &str) -> String;
```

- The existing CLI `parse_context_notes()` **drops** empty entries. The new parser does not; the context-notes caller filters them itself.
- Any shape other than a flat string table is an `Err` for the whole file — including an unquoted dotted key, which TOML reads as a nested table.

- [x] 1.0 Baseline snapshot, `toml` 1.1.6, and one shared parser in `backend`
  - [x] 1.1 **Before any other change**, dump the baseline for Success Metric 1. From the runtime database, export every row where `blurb IS NOT NULL OR context_note IS NOT NULL` as JSON (`sqlite3 -json <db> "SELECT node_uid, blurb, context_note FROM navigation_nodes WHERE …"`) to a file **outside the repository** (the session scratchpad or `/tmp`). Record the path and the two counts here; expected 4,111 summaries and 44 notes.
    - **Recorded:** `/tmp/simsapa-navigation-texts-baseline.json` (914,686 bytes) — 4,111 summaries, 44 notes, out of 12,783 nodes; no empty-string value in either column.
  - [x] 1.2 Add `toml = "1.1.6"` to `backend/Cargo.toml`. Change `cli/Cargo.toml` from `"0.8"` to `"1.1.6"`. Run `cargo check` in `backend` and `cli`. `backend`, `cli` and `bridges` each have their own `Cargo.lock` (there is no workspace); `bridges`' is updated by `make build -B` in 1.7, not by a bare `cargo check`, which needs the Qt environment. `toml` 1.1.6 needs Rust 1.85 (local toolchain: 1.96).
  - [x] 1.3 Confirm the CLI's other `toml::from_str` call sites (`library_imports.rs`, `chanting_practice.rs`) compile and their tests pass under 1.1.6. If the major-version change alters an error or a parse result, report it rather than adapting silently.
    - **Result:** both compile unchanged and their tests pass; no error or parse result differs.
  - [x] 1.4 Create `backend/src/navigation_texts.rs` with `parse_node_texts()` and `unwrap_note()` per the spec above; register the module in `backend/src/lib.rs`. Move `unwrap_note()` verbatim from `cli/src/bootstrap/navigation_tree/mod.rs`.
  - [x] 1.5 Move the parsing tests from `cli/src/bootstrap/navigation_tree/tests.rs` to `backend`: not-a-string-map fails, not-TOML fails, unquoted key fails, hand-wrapped text is joined and keeps its paragraphs. Add one: an entry with `""` is **returned**, with an empty string.
  - [x] 1.6 In the CLI, delete `parse_context_notes()` and `unwrap_note()`; `build_sc_spine` / `build_cst_spine` call `simsapa_backend::navigation_texts::parse_node_texts()` and drop empty entries before `apply_context_notes()`. `CONTEXT_NOTES_TOML` and `apply_context_notes()` stay for now.
  - [x] 1.7 `cd backend && cargo test`, `cd cli && cargo test`, `make build -B` (this is what updates `bridges/Cargo.lock`).

### 2.0 — specs and dependencies

**Depends on:** 1.0 (the parser, for the round-trip test). **Leaves:** two new files in `assets/`; the app unchanged, still reading the database columns.

**What "in the tree" means.** `make_node(node_uid, source_uid, …)` looks a blurb up by `source_uid`. Some nodes are then **elided** by `collapse_redundant_groups` (`long`, `middle`, …). Only nodes in the **final** tree count. An elided node's blurb is therefore used by no node, and is not written.

**Measured on the shipped data (PRD FR-6):** `blurbs.json` has 4,189 English entries. 4,111 are used by a node, exactly one of them under a `node_uid` different from its source uid — `sutta/minor/kn` ← `kn`. The other 78 are used by no node: 7 are re-keyed (FR-7a) and 71 are skipped (FR-7). **The generated file has 4,120 entries**, every key a `node_uid` — the number task 2.10 should reproduce.

**Skip list (PRD FR-7).** Three constants in `blurb_export.rs`, each with a doc comment giving the reason from the PRD's table — this comment is a requirement, not a nicety:

``` rust
/// Super-tree grouping levels. In the Pāli-only scope each holds one member and is
/// elided by `collapse_redundant_groups`, or is never built. …
const SKIPPED_GROUPING_KEYS: &[&str] = &["long", "medium", "linked", "numbered", "minor", "other"];

/// Collections outside the tree's Theravāda Pāli scope (PRD FR-2a of the original
/// feature). Simsapa has no texts for them. …
const SKIPPED_NON_THERAVADA_KEYS: &[&str] = &["da", "da-ot", "ma", /* … 44 in all, listed in PRD FR-7 … */];

/// Per-rule Vibhaṅga blurbs. The tree holds each passage as one ranged leaf, so
/// there is no per-rule node, and each text is identical to the Pātimokkha
/// blurb the tree already shows. (source key, its Pātimokkha counterpart)
const SKIPPED_VIBHANGA_RULE_KEYS: &[(&str, &str)] = &[("pli-tv-bu-vb-as1", "pli-tv-bu-pm-as1"), /* … 21 in all … */];
```

Every `blurbs.json` entry must end up as exactly one of *used by a node*, *re-keyed*, *skipped*. Defects (reported, build continues):

| Defect label | When |
|---|---|
| `blurb for a uid the tree has no node for` | used by no node, not re-keyed, not on a skip list — not written |
| `blurb skip list: key not in blurbs.json` | a stale row |
| `blurb skip list: key is now used by a node` | a stale row; the node gets its blurb as usual |
| `blurb skip list: Vibhaṅga text differs from its Pātimokkha counterpart` | the "redundant" reason no longer holds |

**Key re-mapping (PRD FR-7a).** Seven of those 28 are re-keyed by a hand-maintained table in `blurb_export.rs`, giving nine nodes a summary. Two of the seven are each written under two nodes, which is how 7 source entries become 9 file entries.

``` rust
/// blurbs.json key, the node uids it is written under, a prefix to remove from the text.
const BLURB_KEY_REMAPS: &[(&str, &[&str], Option<&str>)] = &[
    ("sn17.37-43", &["sn17.37", "sn17.38-43"], None),
    ("sn33.51-55", &["sn33.51-54", "sn33.55"], None),
    ("sn39.1", &["sn39.1-15"], None),
    ("an3.156", &["an3.156-162"], None),
    ("sn23.159", &["sn24.1"], Some("sn24.1: ")),
    ("sn28.159", &["sn29.1"], Some("sn29.1: ")),
    ("pli-tv-bu-pm-an", &["pli-tv-bu-pm-ay"], None),
];
```

- A list of named cases only — no inference of any kind.
- A row is **skipped and recorded as a defect** when: the source key is absent from `blurbs.json`; a target is not in the tree; a target already has a blurb under its own uid; the prefix is not at the start of the text. Defect labels: `blurb key remap: source key not in blurbs.json`, `… target node not in the tree`, `… target node has its own blurb`, `… prefix not found`. The blurb of a skipped re-mapping row is not written; it is then also reported as `blurb for a uid the tree has no node for`.
- A re-keyed entry's comment names the source key: `# Vātasutta (blurbs.json key: sn23.159)`. `TreeBlurb` gains `source_key: Option<String>` for this.
- `collect()` takes the table as a parameter so tests can pass their own.

**`TreeNode` change.** Add `pub source_uid: Option<String>`, set in `make_node`. It is what lets the exporter pair a final node with its `blurbs.json` entry. `TreeNode.blurb` stays until task 4.

**File layout** (FR-9, FR-10):

``` toml
# GENERATED — do not edit by hand. …header…

# Dīghanikāya
"dn" = """
The Long Discourses (Dīgha Nikāya, abbreviated DN) is a collection of …
wrapped at a fixed width …
"""

# … one entry per tree node that has a blurb, in tree (depth-first) order.
# Nothing else: every key is a node_uid.
```

- The title in the comment is the node's displayed title (Pāli, else English, else the uid), on **one line** — strip any line break.
- Value form: multi-line basic string. Escape `\` as `\\`, and break any run of three or more `"` with `\"`. Escape control characters. Wrap only at whitespace, and never emit a blank line inside a value unless the source text has a paragraph break — a blank line **is** a paragraph break to the parser.
- Deterministic: no timestamp, no path, no counts in the file.

**Exporter API** (new `cli/src/bootstrap/navigation_tree/blurb_export.rs`):

``` rust
pub struct BlurbExport { pub tree_entries: Vec<TreeBlurb>, pub skipped: usize, pub rekeyed: usize }
pub struct TreeBlurb { pub node_uid: String, pub title: String, pub text: String }

/// Pure. Pairs the final tree with blurbs.json; applies the skip lists and the
/// re-mapping table (both passed in, so tests can supply their own); records defects.
pub fn collect(roots: &[TreeNode], blurbs: &HashMap<String, String>, defects: &mut DefectReport) -> BlurbExport;
/// Pure. The whole file as a string.
pub fn render(export: &BlurbExport) -> String;
/// Writes atomically (temp file in the same folder, then rename). Skips when unchanged.
pub fn write(assets_dir: &Path, content: &str) -> Result<WriteOutcome>;
```

**Path.** `build_sc_spine` gains `assets_dir: Option<&Path>`. The bootstrap passes `<cwd>/../assets` (cwd is `cli/`, resolved the way `bootstrap_assets_dir` is at `cli/src/bootstrap/mod.rs:146`). `build-navigation-tree` gets `--assets-dir`, defaulting to the same. **Guard:** only write if `<assets_dir>/navigation-context-notes.toml` exists (`try_exists()`); otherwise warn and skip, so a wrong working directory never creates a stray file.

- [x] 2.0 Generate `navigation-blurb-suttacentral.toml` in the CLI, and create the overrides file
  - [x] 2.1 Add `source_uid: Option<String>` to `TreeNode` and set it in `sc_spine.rs`'s `make_node`. Check every other place the SC builder creates a node (ranged-rule expansion, `make_leaf`) goes through `make_node`; the CST builder leaves it `None`.
  - [x] 2.2 Make the loaded blurbs reachable from `build_sc_spine` (the builder's `blurbs` map is private today), and make the "nothing loaded" case visible to it: today a missing file fails the builder (`read_json(&path)?`, unchanged by this work) and a file with no `en` section only warns and returns an empty map. The exporter must not write when the map is empty (FR-29).
  - [x] 2.3 Implement `blurb_export::collect()`: depth-first over the final `roots`; a node with a `source_uid` that has a non-empty blurb yields a `TreeBlurb`; Then apply the re-mapping table (FR-7a) with its four checks; a re-keyed entry takes its place in tree order. Then account for every remaining `blurbs.json` entry against the three skip lists and record the four FR-7 defects. Write the skip-list constants with their reason comments, transcribing the 44 and the 21 keys from PRD FR-7.
  - [x] 2.4 Implement `blurb_export::render()` per the layout above, including the generated-file header (FR-10: generated, by which command, do not edit, corrections go in `navigation-blurb-overrides.toml`).
  - [x] 2.5 Tests for `render()` + `parse_node_texts()` round-trip (FR-11) on synthetic input containing: a `\`, single and triple `"`, curly quotes, `<i>` markup, a text long enough to wrap, a text with a paragraph break, and a Pāli title with diacritics. Assert every entry parses back to `unwrap_note(original)`. Add a determinism test: `render()` twice gives identical bytes; entry order is tree order.
  - [x] 2.6 Tests for `collect()`, with small tables passed in: a node whose `node_uid` (`sutta/minor/kn`) differs from its `source_uid` (`kn`) is keyed by `node_uid`; a skipped key (an elided grouping's `long`) is not written and is not a defect; an unused entry on no list is reported and not written; each of the three stale-skip-list defects fires. For the re-mapping: a row writes the text under its target; a two-target row writes both; a prefix is removed; each of the four skip conditions records its defect and leaves the target untouched. Add a test over the **real** constants: no key appears in two lists or in both a skip list and the re-mapping table, and the list sizes are 6, 44 and 21.
  - [x] 2.7 Implement `blurb_export::write()` with the guard and the atomic write, and wire it into `build_sc_spine` **after `replace_spine()` has succeeded**, so a failed tree build never touches the file. Nothing is written when the blurbs map is empty (FR-29) — log a warning and leave the existing file. On a write, log the path, the number of entries written, skipped and re-keyed, and that **the app must be rebuilt** for it to take effect (FR-30).
  - [x] 2.8 Thread `assets_dir` through: `cli/src/bootstrap/mod.rs` (full bootstrap) and `cli/src/main.rs` (`--assets-dir` on `BuildNavigationTree`, with a doc comment).
  - [x] 2.9 Create `assets/navigation-blurb-overrides.toml`: header only, no entries (FR-12). The header explains replace / add / suppress-with-`""`, the always-quote-the-key rule, the wrapping rule, and how to find a `node_uid`. Then run `build-navigation-tree --sc-data-dir …` once to produce `assets/navigation-blurb-suttacentral.toml` (needs ArangoDB — see Notes).
  - [x] 2.10 Inspect the generated file: it parses with `parse_node_texts()`; there are exactly 4,120 entries (the baseline's 4,111 plus the nine re-keyed), every key a `node_uid` in the database; 71 are logged as skipped; no `blurb key remap`, `blurb skip list` or `blurb for a uid the tree has no node for` defect is reported; `sn24.1` and `sn29.1` do not start with their own uid. Record the file size here (PRD T-2 estimates about 0.9 MB).
    - **Recorded:** 847,376 bytes, 4,120 entries; 71 skipped, 7 re-keyed (onto 9 nodes), no defects. Every key is a `node_uid` in the database, the entries are in tree order, and the text of all 4,111 baseline blurbs equals `unwrap_note()` of the stored one — 13 differ from the stored text verbatim, each by a double space only (`an7.42`, `an7.43`, `an9.43`, `mn7`, `mn87`, `sn22.2`, `sn35.133`, `sn35.158`, `sn35.159`, `sn35.245`, `sn36.5`, `sn45.4`, `sn56.37`).
  - [x] 2.11 Run the build a second time and confirm `git status` shows the generated file unchanged (Success Metric 3). `cd cli && cargo test`.
    - **Result:** the second run reports the file unchanged and it is byte-identical (same md5).

### 3.0 — specs and dependencies

**Depends on:** 1.0 (parser), 2.0 (the generated and overrides files must exist — `include_str!` fails the build otherwise). **Leaves:** the app serves both texts from the embedded files; the two columns still exist and are still written, but nothing reads them.

**Statics** (`backend/src/app_settings.rs`, beside `CIPS_GENERAL_INDEX_JSON`):

``` rust
pub static NAVIGATION_BLURB_SUTTACENTRAL_TOML: &str = include_str!("../../assets/navigation-blurb-suttacentral.toml");
pub static NAVIGATION_BLURB_OVERRIDES_TOML: &str = include_str!("../../assets/navigation-blurb-overrides.toml");
pub static NAVIGATION_CONTEXT_NOTES_TOML: &str = include_str!("../../assets/navigation-context-notes.toml");
```

**Cache and resolution** (`backend/src/navigation_texts.rs`):

``` rust
pub struct NavigationTexts { /* three maps; overrides keeps empty entries */ }

impl NavigationTexts {
    /// A file that fails to parse is logged once (naming the file) and treated as empty.
    pub fn from_toml(suttacentral: &str, overrides: &str, context_notes: &str) -> Self;
    /// override (empty = none) → SuttaCentral → none.   FR-20
    pub fn blurb(&self, node_uid: &str) -> Option<&str>;
    pub fn context_note(&self, node_uid: &str) -> Option<&str>;
}

/// Parsed on first call, kept for the life of the process (OnceLock). Logs the parse time.
pub fn navigation_texts() -> &'static NavigationTexts;
```

- `from_toml` takes strings so the resolution is testable without the embedded files.
- **Nothing calls `navigation_texts()` at startup** (FR-18). Its only callers are the three `*_json` functions in `navigation_tree.rs` and the warm-up below.
- **Warm-up (FR-22a).** The window's first query is a synchronous call on the GUI thread, so `SuttaBridge::open_sutta_navigation_window()` starts the parse on a background thread first: `std::thread::spawn(|| { simsapa_backend::navigation_texts::navigation_texts(); });`. `OnceLock::get_or_init` makes a query that arrives early wait for that one initialization. No `qt_thread.queue()` is involved — the thread reports nothing back — so `queue_or_log()` does not apply. On a reopen the cache is already built and the thread returns at once.
- The view is built with an explicit texts argument — `NavigationNodeView::from_node(&node, texts)` — replacing `impl From<&NavigationNode>`. A `From` impl that reached for the global would make every view test depend on the shipped files.
- JSON field names stay `blurb` and `context_note`. QML is not touched.

- [x] 3.0 Embed the three files in `backend` and serve summaries and notes from them
  - [x] 3.1 Add the three statics to `backend/src/app_settings.rs`.
  - [x] 3.2 Implement `NavigationTexts`, `from_toml()`, `blurb()`, `context_note()` and `navigation_texts()` per the spec. Log one `info` line with the elapsed parse time and the three entry counts when the cache is first built.
  - [x] 3.3 Unit tests on `from_toml()` with inline TOML: replace, add (a `cst:` key with no SuttaCentral entry), suppress with `""`, no override falls through to SuttaCentral, unknown uid gives `None`; an unparsable overrides file leaves the other two working (FR-21); an empty context note is `None`.
  - [x] 3.4 Add the guard test (FR-22): each of the three embedded statics parses with `parse_node_texts()`. It must fail — not skip — on a parse error.
  - [x] 3.5 In `navigation_tree.rs`, replace `impl From<&NavigationNode> for NavigationNodeView` with `from_node(node, texts)`, filling `blurb` and `context_note` from `texts` by `node_uid`. Update `NavigationMatchView`'s conversion the same way. `children_json`, `filter_json` and `node_json` pass `navigation_texts()`.
  - [x] 3.5a Add the warm-up to `open_sutta_navigation_window()` in `bridges/src/sutta_bridge.rs`, before the `callback_open_sutta_navigation_window()` call. Change the function body only.
  - [x] 3.6 Update `backend/src/navigation_tree/tests.rs`: view tests pass a `NavigationTexts` built from inline TOML; `a_context_note_reaches_the_view_beside_the_blurb` asserts the texts come from it, not from the row.
  - [x] 3.7 Verify Success Metric 1 against the task 1.1 dump with a temporary `#[ignore]` test (dump path from an environment variable): for every dumped `node_uid`, `texts.blurb(uid) == unwrap_note(dumped blurb)` and the same for the note; and the only uids outside the dump that resolve to a summary are the nine FR-7a targets (`sn17.37`, `sn17.38-43`, `sn33.51-54`, `sn33.55`, `sn39.1-15`, `an3.156-162`, `sn24.1`, `sn29.1`, `pli-tv-bu-pm-ay`). Also count how many dumped blurbs differ from their own `unwrap_note()` form — that is text whose whitespace the new path normalizes. **Expected exactly 13 blurbs and 0 context notes**, each a double space becoming one (PRD FR-2). Any other count, or any other kind of difference, is a stop-and-report. Delete the temporary test afterwards.
    - **Result — summaries:** all 4,111 dumped blurbs resolve to `unwrap_note()` of the stored text; exactly 13 differ from the stored text, each a double space becoming one; the only other uids with a summary are the nine FR-7a targets. As expected.
    - **Result — context notes, a difference to report:** the same 44 keys on both sides and no note is changed by `unwrap_note()`, but **13 notes differ in wording** between the dump and `assets/navigation-context-notes.toml`: `dn`, `mn`, `sn`, `an`, `snp`, `pli-tv-bu-vb`, `pli-tv-bi-vb`, `pli-tv-kd`, `cst:atthakatha/vinayapiṭaka-aṭṭhakathā`, `cst:vin01t2.tik`, `cst:vin02t.tik`, `cst:tika/suttapiṭaka-ṭīkā/majjhimanikāya-ṭīkā`, `cst:tika/suttapiṭaka-ṭīkā/saṃyuttanikāya-ṭīkā`. The database held an older version of those notes than the committed file — it had not been rebuilt since the file's last edit. The app now shows the file's text. The task 2.9 run of the SC spine (old code path, current file) wrote the 8 SC notes to the database identical to the file, which confirms the new path and the old one agree on the current file.
    - The temporary test is deleted.
  - [x] 3.8 Update the header of `assets/navigation-context-notes.toml` (FR-15): the last bullet becomes "rebuild the app to apply an edit; `build-navigation-tree` reports a key that names no node".
  - [x] 3.9 Confirm by reading that nothing on the startup path calls `navigation_texts()` (FR-18): grep for its callers and for the three statics. The only callers are the three `*_json` functions and the warm-up.
  - [x] 3.10 `cd backend && cargo test`, `cd cli && cargo test`, `make build -B`.

### 4.0 — specs and dependencies

**Depends on:** 3.0 (the app must already be off the columns). **Leaves:** the columns gone from the schema, the row structs and the bootstrap; hand-written file keys validated at tree build.

**Validation API** (replaces `apply_context_notes()` in `cli/src/bootstrap/navigation_tree/mod.rs`):

``` rust
/// Reports; writes nothing; never fails the build because of a key.
pub fn validate_node_text_keys(
    rows: &[NewNavigationNode],
    tree_source: &str,
    context_notes: &BTreeMap<String, String>,
    overrides: &BTreeMap<String, String>,      // empty entries kept
    nodes_with_sc_blurb: &HashSet<String>,     // node_uids; empty for the CST spine
    defects: &mut DefectReport,
);
```

| Defect label | When |
|---|---|
| `context note for a node uid not in the tree` | unchanged from today |
| `blurb override for a node uid not in the tree` | an override key this spine owns names no row |
| `empty blurb override for a node with no SuttaCentral blurb` | `""` on a node not in `nodes_with_sc_blurb` |

- **Ownership rule kept** (FR-24): a `cst:`-prefixed key is the CST spine's; everything else the SC spine's. A spine ignores keys it does not own.
- The two hand-written files are read from `backend`'s statics and parsed with `parse_node_texts()`. **A hand-written file that does not parse still fails the tree build**, as today — FR-21's tolerance is the app's, not the bootstrap's.
- `nodes_with_sc_blurb` for the SC spine comes from the task 2 export (`tree_entries`), so it reflects the tree just built, not the embedded copy (PRD T-6).
- Keys of the generated file are not validated (FR-25).

**Migrations.** Both edits are allowed only because neither migration has been released (FR-31). The backend tests apply the real migration files to a temporary database, so they are what proves the edited schema.

- [x] 4.0 Remove the columns and move key validation into the tree build
  - [x] 4.1 **Gate.** Confirm both migrations are unreleased: `git tag --contains $(git log --format=%H -1 --diff-filter=A -- backend/migrations/appdata/2026-09-19-000000_navigation_tree/up.sql)` must print nothing, and the same for the 2026-10-01 folder. If either prints a tag, **stop and raise it** — the answer is then a drop-column migration, not an edit.
    - **Result:** both commands print nothing; the newest tag is `v1.0.0-alpha.8`. Neither migration has been released.
  - [x] 4.2 Remove `blurb TEXT,` from `2026-09-19-000000_navigation_tree/up.sql`. Delete the `2026-10-01-000000_navigation_node_context_note/` folder outright.
  - [x] 4.3 Remove `blurb` and `context_note` from `navigation_nodes` in `backend/src/db/appdata_schema.rs`, and from `NavigationNode` and `NewNavigationNode` in `navigation_tree.rs`. Keep them on `NavigationNodeView`. Update the comment in `tree_is_available_conn` and the one in `with_appdata` to name only the columns that still span migrations (`redirect_uid`).
  - [x] 4.4 Update `INSERT_CHUNK`'s comment in the CLI's `mod.rs` — it states the table has 18 insertable columns.
  - [x] 4.5 In the CLI: remove `TreeNode.blurb` (the exporter reads `blurbs` by `source_uid`), the `blurb` / `context_note` assignments in `flatten_node`, and `CONTEXT_NOTES_TOML`.
  - [x] 4.6 Replace `apply_context_notes()` with `validate_node_text_keys()` per the spec, called from both `build_sc_spine` and `build_cst_spine`. Replace the "N context note(s) applied" log line with counts of keys checked per file.
    - **Note:** `validate_node_text_keys()` returns a `CheckedKeys { context_notes, overrides }` — the keys the spine owned — for the log line. `export_blurbs()` now returns the `node_uid`s that have a blurb in the tree just built, which is the `nodes_with_sc_blurb` argument. When `blurbs.json` yields no English entry that set is empty, so an empty override is then reported as suppressing nothing; the "no blurbs were loaded" warning precedes it.
  - [x] 4.7 CLI tests: rewrite `context_notes_land_on_the_rows_the_file_names` and `a_context_note_for_an_unknown_node_is_reported_per_spine` for the validate-only pass; add the two override defects, the ownership rule for override keys, and a check that a valid key and a non-empty override on any node produce no defect. Remove `the_shipped_notes_file_parses` (covered by the backend guard test, 3.4).
  - [x] 4.8 Backend tests: remove the `include_str!` of the deleted migration and the `blurb` / `context_note` fields from the fixtures in `navigation_tree/tests.rs`. Add a test that a database built from the migration files has neither column (`PRAGMA table_info(navigation_nodes)`).
  - [x] 4.9 FR-34: add a backend test that the level queries still work against a table that **has** the two old columns (apply the migrations, then `ALTER TABLE … ADD COLUMN blurb TEXT` and `context_note TEXT`) — this is every developer database until it is re-bootstrapped.
  - [x] 4.10 `cd backend && cargo test`, `cd cli && cargo test`, `make build -B`.
  - [x] 4.11 Run `build-navigation-tree` with both `--sc-data-dir` and `--cst-xml-dir` against the runtime database. Confirm both spines write, the defect summaries carry no new entries, and `git status` shows the generated file unchanged and **no database staged**.
    - **Result:** `sc_spine` wrote 8,923 rows, checked 30 context note keys and 0 override keys, no defects; `cst_spine` wrote 3,860 rows, checked 14 context note keys and 0 override keys (30 + 14 = the 44 notes). Its one defect, `file imported whole because the split failed: abh05t.nrf.xml`, is the known one in `docs/sutta-navigation-tree.md`. The generated file is reported unchanged and is byte-identical (same md5); no database is staged. The runtime database keeps its two old columns, now NULL on every row, until a full re-bootstrap.
  - [x] 4.12 Success Metric 4: add a misspelled key to each hand-written file (one plain, one `cst:`), rebuild the CLI, re-run 4.11, and confirm each appears in the defect summary of the spine that owns it and in no other. **Revert both edits** and confirm `git diff` on the two files is empty.
    - **Result:** with `sn56.11-typo` and `cst:no-such-book` added to the context notes, and `mn-typo`, `cst:no-such-override` and `"sutta" = ""` to the overrides: `sc_spine` reported `context note for a node uid not in the tree: sn56.11-typo`, `blurb override for a node uid not in the tree: mn-typo` and `empty blurb override for a node with no SuttaCentral blurb: sutta`; `cst_spine` reported the two `cst:` keys and nothing else. Both files reverted; `git diff` on them is empty and the CLI was rebuilt on the reverted files.

### 5.0 — specs and dependencies

**Depends on:** 1.0 – 4.0.

**Docs describe current behaviour.** §2f's comparison table changes shape: the "Column" row goes; the sources become the three files. The CLAUDE.md entry currently says the tree is "built **at bootstrap** into `navigation_nodes` (nothing populates it at runtime)" — still true of the tree, and the entry must now also say the summaries and notes are **not** in it.

**Parse-time measurement.** The number that matters is a **release** build. On desktop, measure with a `cargo test --release` that times `NavigationTexts::from_toml()` on the three statics and prints the elapsed time. On a phone it is read from the `info` line task 3.2 added to `log.txt`.

- [x] 5.0 Documentation, measurement and final verification
  - [x] 5.1 `docs/sutta-navigation-tree.md` §2f (and §2e where it names the column): the three files and which is generated; the one key scheme; the resolution order and the three override actions; that texts are embedded in the app and parsed on the window's first query; the procedure for an edit (edit the file, rebuild the app); where a bad key is reported; that the generated file holds only blurbs a node shows, with the skip lists and the re-keying table in `blurb_export.rs` and what each of their defect reports means; the rule that the bootstrap never writes the overrides file.
  - [x] 5.2 Same doc, §6 (rebuilding): `build-navigation-tree` now rewrites `assets/navigation-blurb-suttacentral.toml`, takes `--assets-dir`, and must be followed by an app rebuild for a changed file to take effect.
  - [x] 5.3 Update the "Sutta navigation tree" entry in `CLAUDE.md` and the lines in `PROJECT_MAP.md` (the `assets/` listing gains two files; `navigation-context-notes.toml`, `navigation_tree.rs`, `bootstrap/navigation_tree/` and "Navigation tree storage" entries lose their column references; add `navigation_texts.rs` and `blurb_export.rs`). Grep `docs/` for `context_note`, `navigation_nodes.blurb` and "re-run `build-navigation-tree`" and fix any stale instruction, including in `docs/navigation-context-notes-research.md`.
  - [x] 5.4 Success Metric 3, final: run the tree build twice; `git status` clean for the generated file after the second run.
    - **Result:** two consecutive runs of both spines; the generated file is reported unchanged both times and is byte-identical (same md5). `git status` shows no change to it.
  - [x] 5.5 Measure the desktop release parse time as specified above. Record the figure and the generated file's size here and for the commit message. Remove the timing test if it asserts nothing.
    - **Recorded:** desktop **release** build, `NavigationTexts::from_toml()` over the three embedded files, five runs: 15.8 ms on the first, 13.0 – 13.6 ms after. Debug build: 116 – 118 ms. Files: `navigation-blurb-suttacentral.toml` 847,376 bytes (4,120 entries), `navigation-context-notes.toml` 16,321 bytes (44 entries), `navigation-blurb-overrides.toml` 1,511 bytes (no entries). The timing test asserted nothing and is removed.
  - [x] 5.6 Full verification: `cd backend && cargo test`, `cd cli && cargo test`, `make qml-test`, `make build -B`. Report any failure with its output; a timing-assertion failure is known drift.
    - **Result:** backend 1,107 passed, 0 failed; CLI 144 passed, 0 failed; `make qml-test` 172 passed, 0 failed; `make build -B` succeeds.
  - [x] 5.7 Review the diff: no `*.sqlite3` staged; no `console` API or new `qWarning()`; no leftover temporary test; no bridge API change (`git diff --stat bridges/src/sutta_bridge.rs` is empty).
    - **Result:** no `*.sqlite3` and no QML file in the branch's diff; no `console` API, `qWarning()`/`qInfo()` or temporary test added. `bridges/src/sutta_bridge.rs` differs from `main` only in the body of `open_sutta_navigation_window()` (the warm-up thread) — no `#[qinvokable]` or `#[qsignal]` added, removed or renamed.
  - [x] 5.8 Hand over the checks that are the user's to do, as a short list in the final report — do not attempt them:
    - Open the Sutta Navigation window: summaries and context notes appear as before, on level rows, tree rows and filter hits; the Summaries toggle still hides and shows them.
    - Success Metric 2: add a replace, an add (on a `cst:` node) and a suppress entry to `navigation-blurb-overrides.toml`, `make build -B` with **no** bootstrap run, and see each in the window. Revert afterwards.
    - On a phone: read the parse-time `info` line from `log.txt` after first opening the window (PRD Open Question 2).
    - Success Metric 5 on the real database: the two columns leave `appdata.sqlite3` only on a **full re-bootstrap** — `build-navigation-tree` does not drop columns from an existing database. The database for the next release must be re-bootstrapped.
    - PRD Open Question 1: decide whether to report the seven mis-keyed blurbs to SuttaCentral.
    - Read the nine re-keyed summaries in the window (`sn24.1`, `sn29.1`, `sn39.1-15`, `an3.156-162`, `pli-tv-bu-pm-ay`, and the two split ranges) and confirm each fits its node.
    - The 13 blurbs that lose a double space (PRD FR-2): confirm that is acceptable.
