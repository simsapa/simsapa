# Tasks: Sutta Navigation Timeline View

PRD: `tasks/2026-10-05-143319-prd---sutta-navigation-timeline-view.md`

## Relevant Files

### Data and backend

- `assets/navigation-context-notes.toml` - The 44 context notes; changes from flat strings to `["uid"]` tables with `note`, `from`, `to`, `circa`.
- `backend/src/navigation_texts.rs` - Embeds and parses the three TOML files; gets the dated-entry struct, the new parser and the date validation. Tests are in the same file.
- `backend/src/navigation_timeline.rs` - **New.** Pure timeline logic: join to node titles, clustering, ordering, period text, HTML rendering. Tests in the same file.
- `backend/src/navigation_tree.rs` - `NavigationNode`, `sort_path`; source of the titles and the tree order. `with_appdata()` is `pub(crate)` so the timeline reads the tree with the same tolerance rules.
- `backend/src/navigation_tree/tests.rs` - Existing context-note tests; their inline TOML fixtures for the context notes (around lines 384 and 729) move to the table shape, their assertions stay as they are.
- `backend/src/html_content.rs` - `sutta_html_page()` and the page template context; has `timeline_html_page()` and the embedded `TIMELINE_CSS`.
- `backend/src/lib.rs` - Module declaration for `navigation_timeline`.
- `cli/src/bootstrap/navigation_tree/mod.rs` - `load_hand_written_texts()` / `validate_node_text_keys()`; parses with the new parser, and `report_context_note_date_defects()` reports bad date bounds per spine.
- `cli/src/bootstrap/navigation_tree/tests.rs` - Tests for the above.
- `bridges/src/api.rs` - Rocket routes; gets `GET /navigation_timeline` and its registration.

### Page assets

- `assets/sass/timeline.scss` - **New.** Styles for the cluster blocks, the axis gutter, the legend and the controls. Compiled by `make sass` to `assets/css/timeline.css`.
- `assets/css/timeline.css` - **New.** The compiled CSS, tracked in git and embedded by `html_content.rs`.
- `src-ts/timeline.ts` - **New.** Entry point of the timeline bundle: reads the cluster blocks, lays them out, draws the SVG with d3.
- `src-ts/timeline_layout.ts` - **New.** Pure layout functions (gaps, scale points, bar spans and lanes, ticks, gutter width, certainty form), no DOM and no d3.
- `src-ts/timeline_layout.test.ts` - **New.** Jest tests for `timeline_layout.ts`.
- `assets/js/timeline.min.js` - **New.** The compiled timeline bundle (d3 included), tracked in git like `simsapa.min.js` and loaded by the timeline page only.
- `webpack.config.js` - Gets a second entry so d3 lands in `assets/js/timeline.min.js`, not in `simsapa.min.js`.
- `package.json`, `package-lock.json` - `d3-scale`, `d3-selection` and their `@types`.
- `src-ts/find.ts` - The find bar; read-only reference (it searches inside `#ssp_content`).

### QML

- `bridges/assets/qml/SuttaNavigationWindow.qml` - The Timeline button and `open_timeline()` beside `open_node()`; the toolbar is a `GridLayout` that takes two rows on mobile.
- `bridges/assets/qml/SuttaSearchWindow.qml` - `show_result_in_html_view()` and the `table_name` switches; gets the focus-existing-tab rule and the new tab type.
- `bridges/assets/qml/SuttaHtmlView_Desktop.qml` - URL dispatch on `table_name` (two places) and `is_sutta_content()`.
- `bridges/assets/qml/SuttaHtmlView_Mobile.qml` - The same dispatch for the native webview.
- `bridges/assets/qml/SuttaTabButton.qml`, `TabListDialog.qml`, `WindowListDialog.qml` - Label a tab with its `item_uid`; read-only reference, no change expected.

### Docs

- `docs/sutta-navigation-tree.md` - §2f (file shape, date convention) and a new section for the timeline page.
- `docs/simsapa-localhost-api-search-endpoints.md` - The route surface; gets the new route.
- `PROJECT_MAP.md`, `CLAUDE.md` - New files and the docs-list entry.

### Notes

- Build with `make build -B`, never a bare `cmake --build` (qmltypes guard).
- Rust tests: `cd backend && cargo test`; CLI tests: `cd cli && cargo test`. Timing-assertion failures in unrelated tests are known drift, not regressions.
- TypeScript tests: `npx jest`; bundle: `npx webpack`; CSS: `make sass`.
- QML: `make qml-test` (lint + tests). Use `Logger`, never `console`. A new warning naming a touched file is what matters.
- Do not run the GUI from an agent shell and never send input to the display. GUI checks are done by hand by the user and are marked **(user)** below.
- No migration, no bootstrap run, no version bump: the data is embedded TOML and a rebuild applies it.
- Do not commit without the user's go-ahead.
- Docs and code comments describe current behaviour; no history narration.
- No new bridge function is planned. If one turns out to be needed, add its stub to `bridges/assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` and use `crate::queue_or_log()` for any `qt_thread.queue()`.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. This helps track progress and ensures you don't skip any steps.

Example:
- `- [x] 1.1 Read file` → `- [x] 1.1 Read file` (after completing)

Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

### 1.0 Context notes as dated tables

**Covers:** FR-1, 2, 7, 8, 9, 10, 11.

**Specs**

``` toml
["dn"]
note = """
Origin: 5th–3rd century BCE, northern India. …
"""
```

``` rust
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextNote {
    pub note: String,           // required; unwrap_note() applied
    pub from: Option<i32>,      // BCE negative
    pub to: Option<i32>,
    #[serde(default)]
    pub circa: bool,
}

pub fn parse_context_notes(toml_text: &str) -> Result<BTreeMap<String, ContextNote>>;

/// One line per defect, e.g. `"dn": from (-200) is later than to (-500)`.
pub fn context_note_date_defects(notes: &BTreeMap<String, ContextNote>) -> Vec<String>;

impl NavigationTexts {
    pub fn context_note(&self, node_uid: &str) -> Option<&str>;          // unchanged signature
    pub fn dated_context_notes(&self) -> impl Iterator<Item = (&str, &ContextNote)>;
}
```

- `deny_unknown_fields` is deliberate: a typo such as `form = -500` must fail the parse, not be dropped silently.
- A file that fails to parse is logged by name and treated as empty at runtime (as today); in the CLI tree build it fails the build (as today).
- An entry whose `note` is empty is dropped, as today.
- The back-reference check of FR-11 is **not** built: the "See …" text names a title, sometimes qualified ("Dīghanikāya (ṭīkā)"), not a `node_uid`, so the target cannot be found reliably. The PRD allows dropping it.

**Depends on:** nothing.

- [x] 1.0 Context notes as dated tables: new TOML shape and parser
  - [x] 1.1 Read `backend/src/navigation_texts.rs` in full and `docs/sutta-navigation-tree.md` §2f.
  - [x] 1.2 Before touching the file, save the current parse for comparison: with a scratchpad Python script (`tomllib`), dump `{uid: note}` of `assets/navigation-context-notes.toml` to a JSON file in the scratchpad directory.
  - [x] 1.3 Convert all 44 entries with a scratchpad script: `"uid" = """` becomes `["uid"]` followed by `note = """`. Keep every comment, section banner and blank line. No dates yet.
  - [x] 1.4 Verify with the scratchpad script that the new file has the same 44 keys and that each `note` is byte-identical to the saved string (FR-7, FR-10).
  - [x] 1.5 Rewrite the file header comment for the new shape: quoted table keys, the `note` field, wrapping rules. The date fields are documented in task 2.
  - [x] 1.6 Add `ContextNote`, `parse_context_notes()` and `context_note_date_defects()` to `navigation_texts.rs` per the specs. Apply `unwrap_note()` to `note`.
  - [x] 1.7 Change `NavigationTexts.context_notes` to hold `ContextNote` values, built with `parse_context_notes()`. Keep the "does not parse → logged by file name, treated as empty" behaviour. `context_note()` returns the note text; add `dated_context_notes()`.
  - [x] 1.8 Tests in `navigation_texts.rs`: a table with only `note`; with one bound; with both and `circa`; `circa` defaults to false; an unknown field is an error; a flat-string entry is an error; an unquoted dotted key is an error; `from > to` is reported and `from == to` is not; the shipped file parses and has 44 entries. Update the existing tests that feed context notes as flat strings (`r#""dn" = 3"#` and the like).
  - [x] 1.9 In `cli/src/bootstrap/navigation_tree/mod.rs`, `load_hand_written_texts()` parses with `parse_context_notes()`. Keep `HandWrittenTexts.context_notes` as the `{uid: note}` map it is today (built from the parsed entries), so `validate_node_text_keys()` and its eight test call sites keep their signature. Add the parsed entries as a second field and report the lines of `context_note_date_defects()` through `defects.record(…)`. The two spines are built by two separate runs (the call sites near lines 469 and 601), so apply the same ownership rule as the key check (`CST_NODE_UID_PREFIX`): each run reports only the entries of its own spine.
  - [x] 1.10 Update `cli/src/bootstrap/navigation_tree/tests.rs` for the new shape and add a test that a `from > to` entry is recorded as a defect and changes nothing else.
  - [x] 1.11 Move the inline context-note fixtures in `backend/src/navigation_tree/tests.rs` to the table shape (`["sn56.11"]` / `note = "…"`); leave every assertion as it is. Run `cd backend && cargo test navigation` and `cd cli && cargo test navigation`; all pass.
  - [x] 1.12 `make build -B` succeeds.

### 2.0 Author the dates

**Covers:** FR-3, 4, 5, 5a, 6.

**Specs — the convention (goes into the file header)**

| Note wording | Fields |
|---|---|
| "5th–3rd century BCE" | `from = -500`, `to = -200` |
| "12th century CE" | `from = 1100`, `to = 1200` |
| "early 5th century CE" | `from = 400`, `to = 430`, `circa = true` |
| "second half of the first millennium CE" | `from = 500`, `to = 1000`, `circa = true` |
| "c. 100 BCE–200 CE" | `from = -100`, `to = 200`, `circa = true` |
| "not before about 300 BCE" | `from = -300`, `circa = true` |
| "5th century CE and later" | `from = 400` (no `to`) |
| "date unknown; older than X" | `to` = X's `from`, `circa = true` |
| "around the turn of the Common Era" | `from = -50`, `to = 50`, `circa = true` |
| "See Dīghanikāya." | the same fields as the entry it refers to |
| no estimate supported by the note or the research doc | no date fields |

- BCE is negative. There is no year 0 in the data; do not write `0`.
- `circa = true` whenever the note says c., about, around, probably, early, late, or gives the bound by inference.
- Never edit `note` text. If a note and the research doc disagree on a date, use the note's wording and list the disagreement for the user — report, do not repair.

**Depends on:** 1.0.

- [x] 2.0 Author the dates
  - [x] 2.1 Read `docs/navigation-context-notes-research.md` and all 44 notes.
  - [x] 2.2 Add the date-field documentation and the convention table (as comments) to the file header.
  - [x] 2.3 Add `from` / `to` / `circa` to every dated entry, section by section. Put the date fields above `note` so they are visible without scrolling past the text.
  - [x] 2.4 Back-reference entries: copy the fields of the entry they refer to. Where it is not certain which entry "See X" means, list it for the user instead of guessing.
  - [x] 2.5 Entries with no supportable estimate get no date fields; list each with the reason.
  - [x] 2.6 Shipped-data tests in `navigation_texts.rs`: `context_note_date_defects()` is empty for the shipped file; no bound is `0`; every bound is within `-600..=2000`.
  - [x] 2.7 Run `cd backend && cargo test navigation_texts` and `cd cli && cargo test navigation`.
  - [x] 2.8 Present to the user, in the chat, a table of all 44 entries: uid, Pāli title, the date wording quoted from the note, `from`, `to`, `circa`; followed by the undated list (2.5) and the uncertain back-references (2.4). **(user)** reviews the numbers before task 3 builds on them.

### 3.0 Timeline data and page route

**Covers:** FR-18, 19, 20, 21, 22, 23, 25, 27 (legend text), 29 (checkbox markup), 38 (wrapping markup).

**Specs**

``` rust
// backend/src/navigation_timeline.rs — no Qt, no HTTP
pub struct TimelineEntry { node_uid, title_pali: Option<String>, title_en: Option<String>,
                           note: String, from: Option<i32>, to: Option<i32>, circa: bool,
                           sort_path: String }
pub struct TimelineCluster { from: Option<i32>, to: Option<i32>, circa: bool,
                             entries: Vec<TimelineEntry> }
pub struct Timeline { clusters: Vec<TimelineCluster>, undated: Vec<TimelineEntry> }

pub fn period_text(from: Option<i32>, to: Option<i32>, circa: bool) -> String;
pub fn build_timeline(entries: Vec<TimelineEntry>) -> Timeline;   // cluster + sort
pub fn timeline_entries_conn(db_conn, texts: &NavigationTexts) -> Vec<TimelineEntry>;
pub fn render_timeline_html(timeline: &Timeline) -> String;
```

Period text (FR-22):

| Fields | Text |
|---|---|
| `-500`, `-200` | `500–200 BCE` |
| `-100`, `200` | `100 BCE – 200 CE` |
| `1100`, `1200` | `1100–1200 CE` |
| `-300`, none | `after 300 BCE` |
| none, `-100` | `before 100 BCE` |
| any of the above with `circa` | prefixed `c. ` |
| none, none | `undated` |
| `from == to` | the single year, e.g. `250 BCE` |

Ordering (FR-21): clusters by sort year (`from`, else `to`); ties by `to` ascending with an absent `to` last; then by the `sort_path` of the cluster's first entry. Entries inside a cluster, and the undated list, by `sort_path`.

Markup (the contract with task 5):

``` html
<div id="timeline" class="timeline">
  <div class="timeline-controls">
    <label><input type="checkbox" id="timeline_scale_toggle"> Scale spacing with time</label>
    <p class="timeline-legend">…three forms in plain words…</p>
  </div>
  <div class="timeline-body">
    <svg class="timeline-axis" aria-hidden="true"></svg>
    <div class="timeline-clusters">
      <section class="timeline-cluster" data-from="-500" data-to="-200" data-circa="false" data-sort-year="-500">
        <h2 class="timeline-period">500–200 BCE</h2>
        <article class="timeline-entry" data-node-uid="dn">
          <h3 class="timeline-title-pali">Dīghanikāya</h3>
          <p class="timeline-title-en">Long Discourses</p>
          <div class="timeline-note"><p>Origin: …</p></div>
        </article>
      </section>
    </div>
  </div>
  <section class="timeline-undated"><h2 class="timeline-period">Undated</h2>…</section>
</div>
```

- An absent bound is an absent attribute, never `data-to=""`.
- Every text is HTML-escaped. A note's paragraphs (`\n\n`) become `<p>` elements.
- The "Undated" section is omitted when there are no undated entries.
- Route: `GET /navigation_timeline?<window_id>` returning the full page. `window_id` is injected as `WINDOW_ID` through `js_extra`, exactly as `render_bold_definition()` does: `suttas.js` and the page menu read it.
- Tree order is `sort_path`. It is unique across both spines in the shipped database (12783 rows, 12783 distinct values), so it is a total order; use `node_uid` as the final tie-break anyway so the output is deterministic on any database.
- Colours: the sass files have **no** theme CSS custom properties for text. A theme is a `body.dark` rule over the sass variables in `assets/sass/_base.sass`. So `timeline.scss` sets no text colour at all (it inherits the body's), expresses "muted" as `opacity`, and the SVG uses `currentColor` only. That is what makes both themes work with no dark-specific rule.
- The page is complete and readable with scripts disabled.
- Performance: the first call to `navigation_texts()` parses the three TOML files (about 27 000 lines). It normally happens when the navigation window opens; with a restored timeline tab it happens in this route instead, on a Rocket worker thread. That is acceptable — it must not be moved to the startup path or called from the UI thread.

**Depends on:** 1.0 (parser), 2.0 (dates; the code works without them, every entry then being undated).

- [x] 3.0 Timeline data and page route
  - [x] 3.1 Read how a non-sutta page with a find bar is built and served: `render_bold_definition()` in `backend/src/html_content.rs` (it ends in `sutta_html_page(&content, None, Some(DICTIONARY_CSS…), Some(js_extra), body_class)`) and the route `get_word_html_q` in `bridges/src/api.rs`. Note where `body_class` comes from (`get_theme_name()` in `backend/src/app_data.rs`, plus the `mobile` class if the word path adds one). `DICTIONARY_CSS` is an `include_str!` of the compiled `assets/css/dictionary.css`, which is tracked in git.
  - [x] 3.2 `{content}` in `assets/templates/page.html` is rendered inside `#ssp_content`, which is where `src-ts/find.ts` searches (verified in review). The page also gets the sutta page menu and the word-lookup hooks of `suttas.js`, as dictionary pages do; that is accepted, not something to strip.
  - [x] 3.3 Create `backend/src/navigation_timeline.rs` with the structs, and declare the module in `backend/src/lib.rs`.
  - [x] 3.4 Implement `period_text()` with a test per row of the table above, including the BCE-to-CE range and `circa`.
  - [x] 3.5 Implement `build_timeline()`: group by `(from, to, circa)`, order per the spec, separate the entries with no bound. Tests: two entries with the same period share a cluster; same `from` but different `to` are two clusters in `to` order; an open-ended cluster sorts after a bounded one with the same sort year; a `to`-only entry sorts by its `to`; entries inside a cluster follow `sort_path`; an entry with no bounds lands in `undated`.
  - [x] 3.6 Implement `timeline_entries_conn()`: load the nodes of all `texts.dated_context_notes()` keys in **one** query (`navigation_nodes::node_uid.eq_any(uids)`), not one query per entry, and take `title_pali`, `title_en` and `sort_path`. A key with no node is skipped with one `warn` log line. Add a test with an in-memory tree, following the fixtures in `backend/src/navigation_tree/tests.rs`. Check which title the navigation row displays (`display_title()` and `NavigationNodeRow.qml`) and use the same fields, so the timeline shows what the panel shows.
  - [x] 3.7 Implement `render_timeline_html()` per the markup spec. Tests: escaping of `<` and `&` in a title and a note; absent bounds produce no attribute; two paragraphs produce two `<p>`; no "Undated" section when the list is empty; an entry without an English title has no `.timeline-title-en` element.
  - [x] 3.8 Create `assets/sass/timeline.scss`: the controls, the legend, a left gutter for the SVG, the cluster and entry typography (period heading clearly a heading, Pāli title prominent and in the Pāli font class used by sutta pages, note as body text), wrapping at phone width with no horizontal scroll. No text colours and no literal colours, per the colour spec above. The file must not start with `_` (a partial is not compiled). Run `make sass`; the compiled `assets/css/timeline.css` is tracked in git like the other compiled CSS.
  - [x] 3.9 Add `timeline_html_page()` to `html_content.rs`, on `sutta_html_page()` with the timeline CSS and no sutta display chrome, following 3.1, with `static TIMELINE_CSS: &str = include_str!("../../assets/css/timeline.css")` passed as `css_extra`.
  - [x] 3.10 Add the `GET /navigation_timeline?<window_id>` route in `bridges/src/api.rs` and register it where the other routes are mounted. It must work when the navigation tree table is absent or empty: the page then shows the controls and a one-line "No timeline data" message.
  - [x] 3.11 `make build -B`; `cd backend && cargo test navigation`.
  - [x] 3.12 **(user)** With the app running, open `http://localhost:<port>/navigation_timeline` in a browser: all entries are listed under date headings in order, undated ones at the end, and the page is unstyled-but-readable on the axis side (no drawing yet).

### 4.0 Open the timeline from the app

**Covers:** FR-12, 13, 14, 15, 16, 17, 35.

**Specs**

``` js
// The one result_data for the timeline. Define the two strings once per file as
// readonly properties; do not scatter the literals.
{ item_uid: "Timeline", table_name: "navigation_timeline",
  sutta_title: "", sutta_ref: "", anchor: "" }
```

- `sutta_title` is empty: the list dialogs and the window title show the uid *and* the title, so a title of "Timeline" reads "Timeline Timeline".
- **The tab label is the `item_uid`.** `SuttaTabButton.qml` shows `elide_long_uid(item_uid)` and the list dialogs follow it, so `item_uid: "Timeline"` is what satisfies FR-15 with no change to those three files. Do not add a label branch for the new type.
- `item_uid` must not be `"Sutta"` or `"Word"` (`is_blank_tab_uid()`), or the tab is treated as blank and dropped from the saved session.
- No sutta has the uid `Timeline` (sutta uids contain `/`), but nothing may look the value up as one: see 4.4.
- Opening follows `SuttaNavigationWindow.open_node()` exactly: `SuttaBridge.emit_show_sutta_from_reference_search("", result_data)` when `count_open_sutta_search_windows() > 0`, otherwise `open_sutta_search_window_with_result(result_data)`. The window raise is done on the C++ side by that path.
- Focus-existing (FR-16) lives in `SuttaSearchWindow.show_result_in_html_view()`, so it holds for every way a timeline result can arrive: if a tab with `table_name === "navigation_timeline"` exists in any of the three tab models, focus it with `focus_on_tab_with_id_key()` and return before a tab is created.
- The view URL is `${api_url}/navigation_timeline?window_id=${window_id}`.

**Depends on:** 3.0.

- [x] 4.0 Open the timeline from the app
  - [x] 4.1 List every site that reads `table_name`: `grep -n "table_name" bridges/assets/qml/*.qml bridges/src/*.rs cpp/*`. For each one in `SuttaSearchWindow.qml` (about 23), `SuttaHtmlView_Desktop.qml`, `SuttaHtmlView_Mobile.qml`, `SuttaHtmlView.qml`, `SuttaStackLayout.qml`, `SuttaTabButton.qml`, `TabListDialog.qml`, `WindowListDialog.qml`, write down what it does for a value it does not know. Most fall through to "treat as a sutta": those are the ones to change.
  - [x] 4.2 `SuttaHtmlView_Desktop.qml`: add a `load_timeline()` that sets the view's URL, a branch for the new `table_name` in **both** dispatch blocks (`Component.onCompleted` and the data-change handler), and make `is_sutta_content()` false for it. Check what `is_sutta_content()` gates and that none of it should run for the timeline.
  - [x] 4.3 `SuttaHtmlView_Mobile.qml`: the same three changes.
  - [x] 4.4 `SuttaSearchWindow.qml`: two sites are written as "everything that is not a dictionary or book type is a sutta" and **will** treat the timeline as one. Both must exclude it: (a) in `show_result_in_html_view()` near line 1371, the block that calls `load_translations_for_sutta(tab_data.item_uid)` and arms the find-on-open query; (b) the `"load-translations"` menu action near line 1126, which should answer "Translations not available". The guard near line 1030 (`table_name !== "suttas"`) already excludes it. Where a site falls back with `|| "suttas"` (lines 412, 449, 487, 1881), the timeline's own non-empty value survives; confirm. History entries (`build_nav_entry`, `open_history_item`) carry `table_name` through and reopen by `item_uid`, so they need no change; confirm.
  - [x] 4.5 Tab title (FR-15): confirm by reading `SuttaTabButton.qml`, `TabListDialog.qml` and `WindowListDialog.qml` that each labels the tab "Timeline" from the `item_uid`, with no change needed.
  - [x] 4.6 Add the focus-existing rule to `show_result_in_html_view()` per the spec, with a small helper `get_tab_with_table_name(table_name)` beside `get_tab_with_id_key()`.
  - [x] 4.7 Session restore (FR-17): `restore_last_session()` replays each saved item through `open_bookmark_in_tab_group()`, not through `show_result_in_html_view()`. Read `open_bookmark_in_tab_group()` (near line 1878) and `get_session_data_json()` and confirm a saved timeline item keeps its `table_name` and reaches the view dispatch of 4.2 / 4.3. If any step looks the item up in the database by `item_uid`, make it pass the timeline through; if it cannot, skip the tab on save instead. The same function serves bookmarks: confirm a bookmarked timeline tab reopens too, or that the bookmark action is not offered for it.
  - [x] 4.8 `SuttaNavigationWindow.qml`: add `open_timeline()` beside `open_node()`, and a `Button { text: "Timeline" }` immediately after `summaries_toggle`, with `font.pointSize: root.pointSize`. Log the open with `logger.info`.
  - [x] 4.9 Check the toolbar still fits at phone width: read the layout around the filter field, Clear, Summaries and Close, and follow whatever wrapping it already uses.
  - [x] 4.10 Find control: the "Find in Page" action (`action_find_in_page`, near line 2247) calls `show_find_bar()` on the current view on both platforms and does not depend on `table_name` or `is_sutta_content()` (which only selects the anchor-jump method). No change expected; confirm the view's `show_find_bar()` works for the timeline in both `SuttaHtmlView_Desktop.qml` and `SuttaHtmlView_Mobile.qml`.
  - [x] 4.11 `make build -B` and `make qml-test`; no new lint warning in the touched files.
  - [x] 4.12 **(user)** Desktop: Timeline button opens a "Timeline" tab and raises the window; the navigation window keeps its state; a second click focuses the same tab; with no Sutta Search window open, one opens with the timeline; quit and restart restores the tab; Ctrl+F finds a title.

### 5.0 d3 drawing and spacing toggle

**Covers:** FR-24, 26, 27, 28, 29, 30, 31, 32, 33, 34, 39.

**Specs**

Layout principle: **the cluster blocks stay in normal document flow.** Spacing is set with `margin-top` on each `.timeline-cluster`; nothing is absolutely positioned except the SVG, which fills the left gutter of `.timeline-body`. This keeps find-bar scrolling and text wrapping working without extra code.

``` ts
// src-ts/timeline_layout.ts — pure, no DOM, no d3
export interface ClusterDates { from: number | null; to: number | null; circa: boolean; sort_year: number; }

/** margin-top for each cluster. Index 0 is always 0. */
export function cluster_gaps(heights: number[], sort_years: number[], scaled: boolean,
                             px_per_year: number, base_gap: number): number[];

/** Strictly increasing years with their y, for a piecewise-linear scale.
 *  Clusters sharing a sort year contribute the LAST one's y. */
export function scale_points(sort_years: number[], marker_ys: number[]): { years: number[]; ys: number[] };

/** Lane (0 = on the axis) for each extent bar so that bars overlapping in y do not overlap in x. */
export function bar_lanes(spans: { y0: number; y1: number }[]): number[];
```

- Even mode: `margin-top = base_gap`. Scaled mode: `margin-top = max(base_gap, px_per_year × Δyears − height of the previous cluster)`, so the distance between two markers is `max(previous height + base_gap, px_per_year × Δyears)`.
- Marker y = the vertical centre of the cluster's `.timeline-period` heading, relative to `.timeline-body`.
- Scale: `d3.scaleLinear().domain(years).range(ys).clamp(true)`, extended by one final point at the latest `to` year, placed at the bottom of the last cluster.
- **A shared sort year maps to the last cluster's marker, and a bar starts at its own marker.** Several clusters share a sort year (`-500 … -300` and `-500 … -200` are two clusters), and the scale can hold only one y per year. With the last cluster's y, `scale(to) >= scale(from) >= own marker y` for every cluster, so no bar can point backwards; with the first cluster's y it can. Add a jest test for exactly this case.
- Extent bar: from the cluster's **own marker y** to `max(scale(to), marker y + 6)` in its lane. Open-ended: a fixed 48 px from the marker — downward for `from`-only, upward for `to`-only — with a fade-out and an arrowhead at the open end.
- Certainty (FR-26): both bounds and not circa → filled marker, solid bar; both bounds and circa → hollow marker, dashed bar; one bound → hollow marker, dashed fading bar with arrow. Strokes and fills use `currentColor` only, with `opacity` for the muted parts; a hollow marker is `fill: none`. No literal colour and no theme-specific rule.
- The SVG has top padding of at least the open-bar length, so a `to`-only bar on the first cluster is not cut off.
- Year ticks (FR-32): scaled mode only, every 100 years inside the domain, as SVG text in the gutter, drawn by hand through the scale (no `d3-axis`). A tick is dropped if it falls within 14 px of the previous one.
- The gutter width is `(lanes used) × lane width + tick label width`, capped so that the text column keeps at least 70% of the page width; beyond the cap, lanes share space.
- `px_per_year`, `base_gap`, the lane width and the open-bar length are named constants at the top of `timeline.ts`.
- **Re-layout only on a width change.** `layout()` changes margins, which changes the height of the observed element; a `ResizeObserver` that re-runs `layout()` on every notification loops forever. Keep the last laid-out width and return early when it has not changed.
- **`timeline_layout.ts` must never import d3.** The d3 packages are ESM-only and the jest setup (`ts-jest`, CommonJS) cannot load them from `node_modules`; keeping the tested module pure is what lets the tests run with no jest configuration change. Only `timeline.ts` imports d3, and it has no jest test.

**Depends on:** 3.0 (the markup contract). Independent of 4.0, but 4.0 is what makes it testable in the app.

- [x] 5.0 d3 drawing and spacing toggle
  - [x] 5.1 Add `d3-scale`, `d3-selection`, `@types/d3-scale` and `@types/d3-selection` as `devDependencies` (where every other package of this project is) with npm.
  - [x] 5.2 `webpack.config.js`: make `entry` an object with `simsapa` and `timeline` and the output filename `[name].min.js`. Run `npx webpack` and confirm with `git diff --stat assets/js/simsapa.min.js` that it is unchanged (d3 must not be in it) and that `timeline.min.js` exists; report its size. `simsapa.min.js` is tracked in git and served from the embedded `assets/` tree, so `timeline.min.js` is tracked the same way and needs a rebuild of the app to be served.
  - [x] 5.3 Load the bundle from the timeline page only: a `<script src="{api_url}/assets/js/timeline.min.js">` added by `timeline_html_page()`. Confirm `tsconfig.json` and the jest config pick up the new files.
  - [x] 5.4 Write `timeline_layout.ts` and `timeline_layout.test.ts`. Tests: even gaps are all `base_gap` after the first; a scaled gap grows with the year difference; a scaled gap never goes below `base_gap` when the previous cluster is taller than the time distance; clusters sharing a sort year produce one scale point; scale years are strictly increasing; two bars overlapping in y get different lanes and two that do not overlap share lane 0.
  - [x] 5.5 `timeline.ts`: read the `.timeline-cluster` elements and their `data-*` attributes into `ClusterDates` (an absent attribute is `null`). If there are none, do nothing.
  - [x] 5.6 Implement `layout()`: measure heights, apply `cluster_gaps()` as `margin-top`, measure marker positions, build the scale, size the SVG to `.timeline-body`.
  - [x] 5.7 Draw with d3 in the SVG: the axis line, one marker per cluster, the extent bars in their lanes, the open-ended form with fade and arrowhead (an SVG `linearGradient` and a `marker` definition), per the certainty table.
  - [x] 5.8 Year ticks in scaled mode only.
  - [x] 5.9 Legend (FR-27): render a small sample of each of the three forms beside its plain-words text in `.timeline-legend`, drawn with the same code as the timeline so the two cannot drift apart.
  - [x] 5.10 The toggle: on change, record the first cluster intersecting the top of the viewport and its offset, re-run `layout()` and redraw, then restore the scroll position so that cluster stays in place (FR-33). No persistence; the checkbox starts unchecked (FR-34).
  - [x] 5.11 Re-run `layout()` on a `ResizeObserver` of `.timeline-clusters` (debounced, width changes only — see the spec) and once on `document.fonts.ready`.
  - [x] 5.12 Add the SVG and gutter styles to `timeline.scss`; `make sass`.
  - [x] 5.13 `npx jest`, `npx webpack`, `make build -B`.
  - [x] 5.14 **(user)** Look at the result in both modes and tune with the implementer: `px_per_year`, the total page length in scaled mode, whether long extent bars crossing many clusters in even mode read well, and the lane widths.

### 6.0 Find bar, themes, phone width, docs

**Covers:** FR-36, 37, 38, 40; PRD §7.4; success metrics.

**Specs**

- The find bar wraps matches in inline `.ssp-find-highlight` spans inside `#ssp_content`. Inline spans do not change block heights, so a find must not trigger a re-layout loop through the `ResizeObserver`; verify rather than assume.
- The SVG tick labels are not find targets. The period headings are, because they are HTML.

**Depends on:** 4.0 and 5.0.

- [ ] 6.0 Find bar, themes, phone width, docs
  - [ ] 6.1 Read `src-ts/find.ts` for how it scrolls a match into view, and confirm nothing in `timeline.ts` (the scroll restore of 5.10, the resize re-layout) fights it. Confirm a find does not cause a re-layout.
  - [ ] 6.2 Confirm the find bar ignores the checkbox label and legend or matches them harmlessly, and that no timeline element is `user-select: none` in a way that hides it from find.
  - [ ] 6.3 Themes (FR-40): grep `timeline.scss` and `timeline.ts` for `#`, `rgb(` and named colours; there must be none, and no `body.dark` rule should have been needed.
  - [ ] 6.4 Phone width (FR-38): confirm in the CSS that the gutter cap of task 5 holds at 360 px, the text column wraps, and nothing sets a fixed width wider than the viewport.
  - [ ] 6.5 Mobile overlay check: the page adds no Qt overlay, but confirm against `docs/mobile-webview-visibility-management.md` that opening the tab from the navigation window (a separate window) leaves the reader webview visible on return.
  - [ ] 6.6 `docs/sutta-navigation-tree.md`: update §2f for the table shape and the date convention, and add a section for the timeline — where the data comes from, the cluster rule, the markup contract between `navigation_timeline.rs` and `timeline.ts`, the flow-layout rule, the separate bundle, and the `table_name` value with the sites that must know it.
  - [ ] 6.7 Add the route to `docs/simsapa-localhost-api-search-endpoints.md`.
  - [ ] 6.8 Update `PROJECT_MAP.md` (new files) and the `sutta-navigation-tree.md` entry in `CLAUDE.md`.
  - [ ] 6.9 Run `make test` (Rust, QML, JavaScript) and `make build -B`; report the results as they are.
  - [ ] 6.10 **(user)** Desktop, both themes: find a Pāli title and an English title in even and in scaled mode; toggle while a find is active; narrow the window to phone width.
  - [ ] 6.11 **(user)** Android (`make android-beta-debug-arm64-install`): Timeline button, tab title, the find control, the toggle, wrapping, dark theme, and returning to the navigation window with its state intact.
