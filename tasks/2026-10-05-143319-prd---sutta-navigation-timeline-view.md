# PRD: Sutta Navigation Timeline View

## 1. Introduction / Overview

The Sutta Navigation window shows each text group (Dīghanikāya, Dhammapada,
Samantapāsādikā, …) with an "Origin:" context note that says when and where the
group was composed. The dates are only readable one group at a time, as prose.

This feature adds a **Timeline** page: a vertical timeline of all the groups
that have a context note, drawn with d3.js and shown in the sutta reader panel
as an ordinary reader tab. It is opened with a **Timeline** button in the Sutta
Navigation window, next to the **Summaries** checkbox.

To plot the groups, the dates that are today written inside the note text of
`assets/navigation-context-notes.toml` are added to the same file as numeric
attributes.

**Goal:** let a reader see at a glance how the collections of the Pāli
literature relate in time, without mistaking an estimated date for a certain
one.

## 2. Goals

1. Every group with a context note appears on one page, in chronological order.
2. Each group shows its time period, Pāli title, English title and origin note,
   matching what the navigation panel shows for that node.
3. The uncertainty of a date (approximate, open-ended, undated) is visible in
   the drawing itself, not only in the text.
4. The reader can switch between even spacing (default) and spacing scaled with
   time.
5. The page's Ctrl+F find bar finds a group by title or note text and jumps to
   it.
6. The dates live in one place, `navigation-context-notes.toml`, as numbers a
   maintainer can edit.
7. The page works on desktop and mobile, offline, in the light and dark themes.

## 3. User Stories

- As a reader, I want to see the Nikāyas, the commentaries and the
  sub-commentaries on one timeline, so that I understand which texts are early
  and which are a thousand years later.
- As a reader, I want an estimated or open-ended date to look different from a
  well-bounded one, so that I do not take a position on the chart as a fact.
- As a reader, I want to press Ctrl+F, type "Milinda" and jump to that group.
- As a reader, I want to switch to time-scaled spacing, so that I can see the
  real gaps between the canonical and the commentarial periods.
- As a maintainer, I want to correct a date by editing two numbers in a TOML
  file and rebuilding the app.

## 4. Functional Requirements

### 4.1 Date data in `navigation-context-notes.toml`

**FR-1.** Each entry in `assets/navigation-context-notes.toml` must change from
a flat string to a table keyed by the same quoted `node_uid`:

``` toml
["dn"]
from = -500
to = -300
note = """
Origin: 5th–3rd century BCE, northern India. Discourses of the Buddha …
"""
```

**FR-2.** The date attributes are:

| Attribute | Type | Meaning |
|---|---|---|
| `from` | integer, optional | Earliest year of origin. BCE is negative (`-500` = 500 BCE). |
| `to` | integer, optional | Latest year of origin. |
| `circa` | boolean, optional, default `false` | The bounds are approximate ("c.", "around", "about", "probably"). |
| `note` | string, required | The existing note text, unchanged. |

**FR-3.** Centuries are written as round numbers: "5th–3rd century BCE" is
`from = -500`, `to = -200`; "12th century CE" is `from = 1100`, `to = 1200`.
"Early", "late" and "second half of" a century take the matching third or half,
rounded, with `circa = true`: "early 5th century CE" is `from = 400`,
`to = 430`. The file header must document this convention and the
BCE-is-negative rule.

**FR-4.** An open-ended date has only one bound:

- "not before about 300 BCE" → `from = -300`, `circa = true`, no `to`.
- "date unknown; older than the Netti" → only `to`, taken from the Netti's
  earliest year, with `circa = true`.

**FR-5.** Every entry must be given at least one bound wherever the note or
`docs/navigation-context-notes-research.md` supports an "after" or "before"
estimate. An entry is left with **no** bounds only when neither source supports
any estimate.

**FR-5a.** A group written over a long period with no stated end ("5th century
CE and later; several authors") is one open-ended entry: `from` only. No end
year is invented for it.

**FR-6.** A back-reference entry ("Origin: See Dīghanikāya.") keeps its note as
written and carries its own `from` / `to` / `circa`, copied from the entry it
refers to.

**FR-7.** The existing note text, including its date wording, must not be
edited as part of this work. The numbers are an addition, not a replacement.

**FR-8.** The parser in `backend/src/navigation_texts.rs` must read the new
shape for the context-notes file. The two blurb files keep the flat
`"key" = "text"` shape and their existing parser.

**FR-9.** A context-notes file that does not parse must behave as today: it is
logged by name and treated as empty; the navigation window still works.

**FR-10.** The navigation window must show exactly the same context note text
as before this change.

**FR-11.** The CLI tree build (`cli/src/bootstrap/navigation_tree/mod.rs`),
which already reports a context-note key that names no node, must keep doing so
with the new shape, and must additionally **report** (never repair):

- an entry where `from` is greater than `to`;
A check that a back-reference entry has the same bounds as its target is
**not** built: the "See …" text names a title, sometimes qualified
("Dīghanikāya (ṭīkā)"), not a `node_uid`, so the target cannot be found
reliably.

### 4.2 Opening the timeline

**FR-12.** The Sutta Navigation window toolbar must have a **Timeline** button
placed immediately after the **Summaries** checkbox, with the same font size as
its neighbours.

**FR-13.** Clicking it must open the timeline in a **new tab** of a Sutta Search
window, the same way `open_node()` opens a leaf: if a Sutta Search window is
open, the tab opens in the last visible one and that window is raised; if none
is open, one is opened with the timeline in it.

**FR-14.** The navigation window must stay open, with its level, breadcrumbs,
filter and scroll position unchanged.

**FR-15.** The tab title must be "Timeline". (Tabs are labelled with their
`item_uid`, so the timeline tab's `item_uid` is `Timeline`.)

**FR-16.** If the target window already has a timeline tab, clicking the button
must focus that tab (and raise the window) instead of opening another. A window
never holds more than one timeline tab.

**FR-17.** Closing the app with a timeline tab open and restarting it must not
break session restore: the tab is either restored as the timeline or skipped,
never restored as an error page or a blank sutta.

### 4.3 The timeline page

**FR-18.** The page is served by the app's localhost API as a full HTML page
built on the same page template as sutta pages, so that it has the app's fonts,
theme colours and the find bar.

**FR-19.** The page lists **every node that has a context note** and no others.

**FR-20.** Entries with the **same period** — identical `from`, `to` and
`circa` — form one **cluster**. A cluster is drawn as:

1. one **date heading**: the time period, as text generated from the numbers
   (FR-22), shown once;
2. below it, each entry of the cluster in turn:
   - the **Pāli title** of the node;
   - the **English title** of the node, if it has one;
   - the **origin note**, in full, as shown in the navigation panel.

An entry whose period no other entry shares is a cluster of one and is drawn
the same way. The titles are the ones the navigation panel shows for that node.
A cluster has **one** marker and **one** extent bar on the axis, at its date
heading.

**FR-21.** Clusters are ordered by their **sort year**: `from` if present,
otherwise `to`. Clusters with the same sort year are ordered by `to` (an absent
`to` last), then by the navigation-tree order of their first entry. Inside a
cluster, entries keep navigation-tree order.

**FR-22.** The time period text is generated from the attributes:

| Attributes | Text |
|---|---|
| `from` and `to` | `500–200 BCE`, `100 BCE – 200 CE`, `1100–1200 CE` |
| `from` only | `after 300 BCE` |
| `to` only | `before 100 BCE` |
| `circa = true` | the above prefixed with `c.` |
| neither bound | `undated` |

**FR-23.** Entries with no bounds are listed under an **"Undated"** heading below
the timeline, with the same title and note content and no marker, so that Ctrl+F still finds
them.

**FR-24.** The layout is a single vertical axis with one marker per cluster and
the cluster's heading and entries beside it. Unlike the reference image, it has no coloured
labels and no white background; it uses the page's theme colours.

**FR-25.** The timeline is read-only: clicking an entry does nothing.

### 4.4 Showing uncertainty

**FR-26.** The drawing must distinguish three levels of certainty, without
relying on colour alone:

| Case | Marker | Extent bar |
|---|---|---|
| Both bounds, `circa = false` | filled | solid, from `from` to `to` |
| Both bounds, `circa = true` | hollow (outline only) | dashed, from `from` to `to` |
| One bound only | hollow | dashed, fading out on the open side, ending in an arrowhead that points to the open side |

**FR-27.** A short legend at the top of the page must explain the three forms in
plain words, e.g. "filled: dated range · hollow: approximate · arrow: only an
earliest or latest date is known".

**FR-28.** The extent bar is drawn along the axis in **both** spacing modes.

### 4.5 Spacing toggle

**FR-29.** At the top of the page there must be a checkbox labelled **"Scale
spacing with time"**, unchecked by default.

**FR-30.** Unchecked: clusters are evenly spaced, one after another.

**FR-31.** Checked: the vertical distance between two consecutive clusters grows
with the number of years between their sort years, so that long gaps in time
are long gaps on the page. Clusters must never overlap: the distance is never
smaller than the room the cluster's heading and entries need (see §7.3). The
entries inside a cluster are spaced the same in both modes.

**FR-32.** In scaled mode the axis shows year tick labels (e.g. every century)
so the reader can read the scale.

**FR-33.** Toggling re-lays out the page in place, without a reload, and keeps
the cluster that was at the top of the viewport in view.

**FR-34.** The checkbox state is not persisted; the page always opens unchecked.

### 4.6 Find bar

**FR-35.** The page must include the same find bar as sutta pages, opened with
Ctrl+F on desktop and by the app's usual find control on mobile.

**FR-36.** The find bar must match and highlight text in the titles, the time
period text and the notes, and scroll the match into view.

**FR-37.** Matches must still be found after the spacing toggle has re-laid out
the page.

### 4.7 Platforms and packaging

**FR-38.** The feature must work on desktop and on mobile. At phone width the
entry text wraps and the page has no horizontal scroll.

**FR-39.** d3.js must be added as an npm dependency and bundled with webpack
into the app's assets. The page must not load anything from the network.

**FR-40.** The page must be readable in both the light and the dark theme.

## 5. Non-Goals (Out of Scope)

- Clicking a timeline entry to reveal the node in the navigation window or to
  open a text.
- Zooming, panning, or a horizontal timeline.
- Filtering or grouping by Piṭaka / Aṭṭhakathā / Ṭīkā.
- Showing the SuttaCentral summaries (blurbs) on the timeline.
- Persisting the spacing checkbox.
- Rewriting or regenerating the note text from the numbers.
- Adding dates for nodes that have no context note.
- Moving the dates into the `navigation_nodes` table. They stay in the embedded
  TOML, so no bootstrap run and no database version change is needed.
- Historical events (councils, Aśoka, etc.) as timeline markers.

## 6. Design Considerations

- **Reference image:** a vertical line with round markers and text blocks beside
  them. Simplified here: one column of text to the right of the axis (not
  alternating sides), which is what fits a phone.
- **Cluster text order:** the date heading first, then per entry the Pāli title
  (prominent), English title, and the note as body text. The date heading must
  read as a heading for everything under it, clearly distinct from an entry
  title.
- **Fonts:** the Pāli title uses the same font class the sutta pages use for
  Pāli text, so the diacritics render with the patched Crimson Pro.
- **Controls:** the checkbox and the legend sit above the timeline and scroll
  away with the page.
- **Colour:** axis, markers and bars take the theme's text and muted colours.
  Certainty is carried by fill, dash and arrow (FR-26), never by hue.

## 7. Technical Considerations

### 7.1 Data path

- `backend/src/navigation_texts.rs` holds the three embedded TOML files and
  parses them on first use. Add a context-note struct (`note`, `from`, `to`,
  `circa`) and a separate parse function for that file; `NavigationTexts::context_note()`
  keeps returning the note text, and a new accessor returns the dated entries.
- `parse_node_texts` is also called on the context-notes file in
  `cli/src/bootstrap/navigation_tree/mod.rs` and its tests. Those call sites
  must move to the new parser.
- `unwrap_note()` must still be applied to `note`.
- The titles come from `navigation_nodes`, joined to the dated entries by
  `node_uid`. A context-note key with no node is skipped on the page (the tree
  build already reports it).
- The parse stays off the startup path, as today.

### 7.2 Serving and opening the page

- A new Rocket route in `bridges/src/api.rs` (e.g. `GET /navigation_timeline`)
  renders the page from `assets/templates/page.html`, which already includes the
  find bar (`{find_html}`). The server renders the clusters and entries as HTML,
  with the dates as `data-*` attributes for the script to read, so the page is
  complete and searchable before any script runs.
- The reader tab is opened from QML with a `result_data` object, as
  `open_node()` does with `table_name: "suttas"`. `SuttaHtmlView_Desktop.qml`
  and `SuttaHtmlView_Mobile.qml` choose the URL from `table_name`; the timeline
  needs a new `table_name` value (e.g. `"navigation_timeline"`) handled in both.
- **To verify during implementation, before building on it:**
  1. every place that switches on `table_name` (tab title, find, prev/next,
     translations bar, session save/restore, the mobile `is sutta` checks)
     behaves sensibly for the new value — list them with
     `grep -rn "table_name" bridges/assets/qml cpp bridges/src`;
  2. session restore (FR-17) with a tab whose `item_uid` is not a database row;
  3. how an existing tab is found and focused (FR-16) — look for a "tab already
     open" lookup in `SuttaSearchWindow.qml` before writing one.
- If a new bridge function is added, add its stub to
  `bridges/assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`.
- Use `Logger` in QML, never `console`.

### 7.3 Rendering

- New TypeScript module in `src-ts/` (e.g. `timeline.ts`), bundled by webpack.
  Check the effect of d3 on the size of `simsapa.min.js`, which every sutta page
  loads: the timeline is a separate webpack bundle (`timeline.min.js`) loaded
  only by this page, importing `d3-scale` and `d3-selection`.
- **Entry text must be ordinary HTML elements, not SVG `<text>`.** The find bar
  (`src-ts/find.ts`) works on the page's HTML text, and HTML wraps at phone
  width for free. d3 draws only the axis, markers, extent bars and ticks, in an
  SVG that sits beside the column of entry blocks.
- **Clustering, ordering and the period text are done in Rust**
  (`backend/src/navigation_timeline.rs`); the cluster key is `(from, to, circa)`.
  The script only lays out and draws.
- **One scale for both modes.** Lay the cluster blocks out first, then build a
  piecewise-linear d3 scale from each cluster's sort year to its marker's y
  position. Extent bars and ticks are drawn through that scale, so they are
  correct in either mode. The two modes differ only in how the gap is computed:
  - even: gap = the cluster block's height + a fixed margin;
  - scaled: gap = max(the cluster block's height + margin, `k` × years to the
    next cluster), with `k` a constant tuned so the whole page stays a
    reasonable length (the data spans roughly 500 BCE to the second millennium
    CE).
- A large cluster (most of the early canon shares one period) is tall, and the
  scale is stretched across it. Clusters with the same sort year but different
  periods sit one after another. This is why the spacing is "scaled", not a
  strictly linear axis.
- The piecewise scale needs strictly ordered sort years to interpolate between;
  for clusters that share a sort year, use the **last** one's y for that year,
  and start each extent bar at its own marker — otherwise a bar can point
  backwards.
- An open-ended bar (FR-26) has no second year to draw to: give it a fixed
  visual length.
- Entry heights depend on the width, so re-run the layout on resize and after
  fonts load.
- Unit tests: clustering, sort order and period text (FR-22) in Rust; the gap
  computation, scale points and bar lanes in jest (`timeline_layout.test.ts`),
  in a module that does not import d3 (the d3 packages are ESM-only and the
  jest setup cannot load them).
- Re-layout on width changes only: the layout changes the height of the element
  the `ResizeObserver` watches.
- The app's themes are `body.dark` rules over sass variables, not CSS custom
  properties: the timeline inherits the body text colour and the SVG uses
  `currentColor`, so it needs no theme-specific rule.

### 7.4 Tests and docs

- Rust tests: the new parser (table shape, optional bounds, `circa` default, a
  malformed file is ignored as a whole); the shipped file parses; every shipped
  entry has `from <= to` when both are present.
- Update `docs/sutta-navigation-tree.md` §2f (file shape, date convention),
  `PROJECT_MAP.md`, and the docs list in `CLAUDE.md`.
- No migration, no bootstrap run and no database version change: the data is
  embedded in the app.

## 8. Success Metrics

1. All 88 context-note entries appear on the page: under a date heading on the
   axis if they have a bound, under "Undated" otherwise.
2. Every entry's bounds can be justified from its note text or the research
   doc; a reviewer reading the TOML finds no number without a source.
3. Ctrl+F for a group's Pāli or English title jumps to it, in both spacing
   modes.
4. The navigation window shows byte-identical context notes before and after.
5. The page is usable at phone width on Android and in a narrow desktop window,
   in both themes.
6. `make test` passes; `make qml-lint` shows no new warning in touched files.
7. Sutta pages do not load noticeably slower because of the d3 dependency.

## 9. Open Questions

None open.

### Decided

- Mobile find control: the "Find in Page" action calls `show_find_bar()` on the
  current view on both platforms and does not depend on the tab type, so the
  timeline tab gets it with no change (FR-35).
- The back-reference bounds check is not built (FR-11).

- Clustered entries show the date heading once, followed by the entries
  (FR-20).
- Round numbers for centuries; thirds and halves for "early / late / second
  half of" (FR-3).
- A group written over a long period is one open-ended entry (FR-5a).
- A second click on Timeline focuses the existing tab (FR-16).
