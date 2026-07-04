# PRD: DPD EPD Word-List Links (trigger a Combined dictionary lookup on click)

## 1. Introduction/Overview

DPD (Digital Pāḷi Dictionary) English→Pāḷi reverse-lookup pages — e.g.
`http://localhost:4848/get_word_html_by_uid/window_0/happy/dpd` — render a list
of Pāḷi equivalents as plain bold text:

```html
<b class=epd>attamana</b> adj. pleased; happy; delighted; elated<br>
<b class=epd>abhiraddha</b> pp. satisfied; pleased; happy (with) (+instr)<br>
...
```

Today these `<b class=epd>WORD</b>` items are not clickable. A user reading the
`happy/dpd` page cannot jump from `attamana` to its DPD entry.

This feature adds a **bootstrap-time text-processing step** that rewrites every
`<b class=epd>WORD</b>` item into an internal `ssp://` link. Clicking the word
triggers a **Combined dictionary lookup** for that word, showing the results in
the dictionary sidebar (the same "Combined" mode the search bar offers: a DPD
lookup together with the word deconstructor).

Using a Combined lookup for **all** epd words (rather than resolving each word to
a specific entry uid) deliberately sidesteps the resolution complications:

- A word with a **single** DPD entry → the lookup shows that one entry.
- A word with **multiple** DPD entries → the lookup shows all of them and the
  user picks the right one.
- A word with **no** DPD entry → the lookup can still surface matches from other
  dictionaries.

So no per-word existence check or single-vs-multiple counting is needed at
bootstrap: every epd word becomes the same kind of link.

This mirrors the existing DPPN cross-reference rewrite
(`ssp://dppn_lookup/<encoded>` → `run_dppn_dictionary_query`) and the DPD
example-sutta-link rewrite (`convert_dpd_example_sutta_links()` in
`backend/src/db/dpd.rs`).

**Goal:** make EPD word lists navigable by making each Pāḷi equivalent trigger a
Combined dictionary lookup, reusing the app's existing `ssp://` link +
sidebar-lookup infrastructure.

## 2. Goals

1. During CLI bootstrap, rewrite every `<b class=epd>WORD</b>` item in DPD
   `dict_words` rows into a clickable internal link that triggers a **Combined**
   dictionary lookup for `WORD`.
2. Use a single uniform link form for all epd words — no existence check, no
   single-vs-multiple distinction.
3. Add the front-end (`src-ts/helpers.ts`) link-classification branch for the new
   `ssp://` word-lookup link form.
4. Add the backend API route + callback that carries the Combined-lookup signal
   to the reader panel (mirroring `POST /dppn_lookup` →
   `callback_run_dppn_dictionary_query`).
5. Add the QML handler that runs a Combined dictionary query in the sidebar
   (mirroring `run_dppn_dictionary_query`).
6. Do not regress the existing sutta-link / DPPN rewrite, the
   FTS5/`definition_plain` fields, or search behavior.

## 3. User Stories

- **As a reader browsing an English→Pāḷi (EPD) DPD page** (e.g. `happy/dpd`), I
  want to click a Pāḷi equivalent like `attamana` and have the app look it up in
  the dictionary sidebar, so I can read its definition without manually typing a
  search.
- **As a reader clicking a Pāḷi word with several DPD entries**, I want the
  Combined lookup to show all matching entries so I can choose the correct one.
- **As a reader clicking a Pāḷi word not covered by DPD**, I want the Combined
  lookup to still search the other dictionaries so I have a chance of finding it.

## 4. Functional Requirements

### Bootstrap rewrite (CLI / `backend/src/db/dpd.rs` + `helpers.rs`)

1. The system must add a bootstrap step that scans DPD `dict_words` rows
   (`dict_label = 'dpd'`) whose `definition_html` contains `<b class=epd>` items
   and rewrites each `<b class=epd>WORD</b>` item into a Combined-lookup link.
   Approximately 77,850 rows contain `epd` markup, so the step must run
   efficiently (batched, in a transaction, **before** the dictionaries FTS5
   indexes/triggers are created — same ordering as the existing
   `convert_dpd_example_sutta_links()`).
2. Each `<b class=epd>WORD</b>` item must be rewritten to a link that carries a
   **distinct CSS class** so linked words are visually distinguishable (the same
   way DPD example-sutta links use `a.sutta_link`). The proposed markup is
   `<a class="epd word_link" href="ssp://word_lookup/<encoded-word>">WORD</a>`,
   where `<encoded-word>` is the percent-encoded `WORD` (matching the encoding
   scheme used for `ssp://dppn_lookup/<encoded>`). The original visible text
   (`WORD`) must be preserved. (The exact class name — e.g. `word_link` /
   `epd_link` — and whether to keep the `epd` class or replace it is an
   implementation detail; the requirement is a dedicated class the new CSS in
   FR-6 targets.)
3. The rewrite must apply uniformly to **every** epd word — no existence check
   and no counting of DPD entries. (The Combined lookup resolves single,
   multiple, and zero-match cases at click time.)
4. The rewrite must be **idempotent-safe** for re-bootstrap: re-running must not
   double-wrap already-linked items (guard by only matching bare
   `<b class=epd>WORD</b>` items, or by skipping items already containing an
   `href="ssp://word_lookup/`).
5. After rewriting `definition_html`, the system must keep `definition_plain`
   consistent so the added `<a>` markup does not leak into `definition_plain`
   (recompute plain text the same way the sutta-link step does via
   `compact_rich_text`; the visible word text must remain in plain text so
   search still matches it).
6. The system must add custom CSS for the new word-link class to the DPD
   stylesheet `assets/dpd-res/dpd-css-and-fonts.css` (and any sass source, if
   one drives that file), giving linked epd words a distinct look plus a
   `:hover` state — analogous to the existing `a.sutta_link` /
   `a.sutta_link:hover` rules in that file. The styling should stay legible in
   both light and dark themes (reuse the `--primary*` CSS variables the
   stylesheet already uses).
7. The word carried in the link's query must be the epd word **with diacritics
   exactly as given in the item** (already niggahīta-normalized ṃ→ṁ after DPD
   migration); no additional normalization is required beyond percent-encoding
   for the URL.

### Front-end link handling (`src-ts/helpers.ts`)

8. `handle_link_click` must classify `ssp://word_lookup/<encoded>` links: on
   click, `event.preventDefault()`, decode the word, and call a new
   `run_word_lookup(word)` that issues `POST /word_lookup` with
   `{ window_id, query }` — mirroring the existing `run_dppn_lookup` →
   `POST /dppn_lookup` helper.
9. The new branch must be ordered so it does not collide with the existing
   `ssp://suttas/...` and `ssp://dppn_lookup/...` handling.
10. Export the new helper alongside the existing `run_dppn_lookup` export and
    rebuild the TS bundle (`npx webpack`).

### Backend API (`bridges/src/api.rs`)

11. Add `POST /word_lookup` (request `{ window_id, query }`) that emits a new
    FFI callback `callback_run_combined_dictionary_query(window_id, query)`,
    mirroring `dppn_lookup` → `callback_run_dppn_dictionary_query`.
12. Declare the new callback in the CXX bridge and register the new route in the
    Rocket route list.

### QML / reader panel (`assets/qml/SuttaSearchWindow.qml`)

13. Add `run_combined_dictionary_query(query)` mirroring
    `run_dppn_dictionary_query(query)`: reveal the side panel, activate the
    Results tab (idx 0), switch the search area to **Dictionary**, set the search
    mode to **Combined**, populate the input with the word, and run the query.
    It must **ensure the dictionary filter settings needed for the lookup** are
    in place (e.g. so DPD and the other dictionaries the Combined lookup relies
    on actually contribute — unlike the DPPN handler, which solo-locks DPPN, this
    one should set the filter to whatever the Combined lookup requires rather than
    a single-dictionary lock).
14. Wire the new callback so the C++/CXX-Qt side routes
    `callback_run_combined_dictionary_query` to
    `SuttaSearchWindow.run_combined_dictionary_query` (same wiring path used for
    the DPPN query callback).

## 5. Non-Goals (Out of Scope)

1. Linking any DPD markup **other than** `<b class=epd>` word-list items (no
   compound/family/root cross-reference linking in this PRD).
2. Resolving each epd word to a specific entry uid or opening a word directly in
   a new tab (the earlier `/open_word_tab` / exact-uid approach is explicitly
   dropped in favor of the uniform Combined lookup). No per-word existence check.
3. Runtime (in-app) rewriting of user-imported dictionaries — this is a
   **bootstrap-only** change for the shipped DPD data.
4. Changing the "Combined" search mode's ranking/behavior, or the existing
   sutta-link / DPPN rewrite behavior.

## 6. Design Considerations

- Visual: linked epd words **should look distinct** from surrounding text (this
  is a wanted change, not a preserved look), exactly as DPD example-sutta links
  are styled via the dedicated `a.sutta_link` / `a.sutta_link:hover` rules in
  `assets/dpd-res/dpd-css-and-fonts.css`. Add a parallel rule for the new
  word-link class in the same file, reusing the `--primary*` CSS variables so it
  reads correctly in light and dark themes. Note the existing `.epd` /
  `.epd:hover` rules already exist in that file (and a `b.epd a` rule in
  `assets/css/dictionary_old.css`); the new class should give a clear,
  intentional link affordance rather than relying on those.
- Link form `ssp://word_lookup/<encoded>` parallels the existing
  `ssp://dppn_lookup/<encoded>` convention.

## 7. Technical Considerations

- **Reference implementations to mirror:**
  - Rewrite shape: `convert_dpd_example_sutta_links()`
    (`backend/src/db/dpd.rs:808`) — batched `dict_words` scan (`id > ? ... LIMIT
    ?`), per-batch transaction, run before FTS5 indexes exist so sync triggers
    don't fire, `definition_plain` recomputed.
  - Lookup-trigger plumbing: `run_dppn_lookup` (`src-ts/helpers.ts`) →
    `POST /dppn_lookup` (`bridges/src/api.rs:375`) →
    `callback_run_dppn_dictionary_query` (`api.rs:224`) →
    `SuttaSearchWindow.run_dppn_dictionary_query` (`SuttaSearchWindow.qml:1330`).
    The new Combined path is a near-copy of each of these.
- **Called from** the DPD bootstrap in `cli/src/bootstrap/dpd.rs`
  (`dpd_bootstrap()`), sequenced alongside / after
  `convert_dpd_example_sutta_links()` and **before**
  `create_dictionaries_fts5_indexes()`.
- **Combined mode:** the search bar already exposes a "Combined" Dictionary mode
  ("runs a DPD lookup together with the word deconstructor" — see
  `SearchHelpWindow.qml` / `SearchBarInput.qml`). The QML handler just needs to
  select it. Note the documented **Combined → DpdLookup remap** for the
  `/dict_combined_search` route (see
  `docs/simsapa-localhost-api-search-endpoints.md`) — the QML path drives the
  search bar UI (like `run_dppn_dictionary_query`) rather than calling that route
  directly, so the remap is not a concern here, but keep it in mind.
- **`definition_plain` integrity:** the epd word text must remain present in the
  plain field (only the `<a>` wrapper is stripped) so ContainsMatch/FulltextMatch
  over EPD pages still finds the word.
- **Re-bootstrap required:** because FTS5 tables are rebuilt by SQL scripts and
  the DBs are shipped pre-built, verifying this feature requires re-running the
  affected DPD bootstrap step and pointing the app at the regenerated DBs.
- **Performance:** ~77,850 candidate rows; keep the rewrite batched. Because
  there is no per-word DB lookup, the step is a pure in-memory HTML transform per
  row plus the batched UPDATE.
- Follow CLAUDE.md procedures for any new QML type-stub function needed for
  `qmllint` and for registering the new Rocket route / CXX callback.

## 8. Success Metrics

1. On a freshly bootstrapped DB, the `happy/dpd` page renders every listed Pāḷi
   equivalent (`attamana`, `abhiraddha`, …) as a clickable link.
2. Clicking `attamana` reveals the sidebar and runs a **Combined** dictionary
   lookup for `attamana`, showing its DPD entry (or entries) in the results.
3. Clicking a word with multiple DPD entries shows all of them in the results;
   clicking a word absent from DPD still runs the Combined lookup (surfacing
   other-dictionary matches where available).
4. Search snippets/highlighting and `definition_plain` are unchanged by the
   rewrite (no `<a ...>` leakage into plain text; the word text still matches in
   search).
5. Existing sutta-link and DPPN rewrites and their tests continue to pass.

## 9. Resolved Decisions

These were raised as open questions and have been decided:

1. **Combined query input:** pass the epd word **with diacritics exactly as
   given in the item** (no case/diacritic normalization beyond percent-encoding).
2. **Sidebar dictionary filter:** the handler must **ensure the correct filter
   settings needed for the Combined lookup** (so DPD and the other relevant
   dictionaries contribute) — it should not simply leave the user's current
   filter untouched, and it should not solo-lock a single dictionary the way the
   DPPN handler does.
3. **Visual treatment:** linked words **do** get a distinct look via a dedicated
   CSS class + custom CSS (like `a.sutta_link`), not the plain `.epd` styling.
4. **Route/callback naming:** `POST /word_lookup` +
   `callback_run_combined_dictionary_query` are confirmed.

## 10. Open Questions

1. **New CSS class name:** exact name for the word-link class (e.g. `word_link`
   vs `epd_link`) and whether it replaces or augments the existing `epd` class —
   an implementation detail to settle during the CSS work.
2. **Exact "correct filter settings":** the precise Dictionaries-panel state the
   Combined lookup needs (which locks/toggles to set) should be confirmed against
   the current Combined-mode behavior when implementing
   `run_combined_dictionary_query`.
