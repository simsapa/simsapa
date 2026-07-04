# PRD: Header & Footer Removal for Plain-Text Indexing (Bootstrap)

- **Date:** 2026-07-04
- **Status:** Draft
- **Area:** Bootstrap plain-text generation (`content_plain`, `definition_plain`)

## 1. Introduction / Overview

During the bootstrap procedure, the app converts each sutta and dictionary
entry from HTML/JSON into a **plain-text** form that is stored in
`suttas.content_plain` and `dict_words.definition_plain`. These fields feed the
ContainsMatch (SQLite FTS5) and FulltextMatch (Tantivy) search indexes, so any
text in them becomes matchable search content.

Today, **header and footer boilerplate leaks into these plain-text fields**.
Chapter/collection headers (nikāya, vagga, division, subdivision), publication
footers, and dictionary boilerplate (inflection-table notes, feedback prompts,
"loading…" placeholders) end up indexed. This pollutes search results — a user
searching for a common collection word can match hundreds of suttas purely
because every sutta's header repeats the nikāya/vagga name, and dictionary
searches match on footer boilerplate rather than the actual definition.

**Goal:** Make `content_plain` and `definition_plain` contain only the
*meaningful body text plus the entry's own title*, with collection headers and
publication/boilerplate footers reliably removed — across the three affected
content types — and regenerate the shipped databases via re-bootstrap.

## 2. Goals

1. Reliably strip **header** boilerplate (nikāya / vagga / division /
   subdivision names) from sutta `content_plain`, while **preserving the sutta
   title, including any leading reference number**.
2. Reliably strip **footer** boilerplate (publication notes, credits, license,
   editor lines) from sutta `content_plain`.
3. Reliably strip **dictionary footer boilerplate** from DPD
   `definition_plain` — everything from the "Inflections not found in any Pāḷi
   corpus…" paragraph onward (inflection note, feedback / "Report it here"
   block, and "…loading…" placeholder divs) — while keeping the definition body
   and the declension forms above that boundary.
4. Fix the existing correctness bug where multi-line `<header>` blocks are not
   removed at all (the regex lacks a DOTALL flag).
5. Cover all three content paths: SuttaCentral HTML suttas, Bilara JSON suttas,
   and the DPD dictionary.
6. Regenerate the shipped `appdata.sqlite3` and `dictionaries.sqlite3` via
   re-bootstrap so the improvements take effect.

## 3. User Stories

- *As a reader searching Pāli text,* when I search for a word, I want results
  that actually contain that word in the sutta body or title — not suttas that
  merely repeat a nikāya/vagga name in their header — so my results are
  relevant.
- *As a reader,* when I search for a sutta by its title (e.g. `araññasutta`,
  `harita-mata jātaka`), I still want that sutta to be found, because the title
  is preserved in the indexed text.
- *As a reader searching the dictionary,* when I search for a word, I want to
  match on the definition and the word's inflected forms — not on generic
  footer text like "inflections not found in any Pāḷi corpus" or "Report it
  here" that appears on every entry.
- *As a maintainer,* I want the header/footer removal logic to be centralized
  and testable, so future content sources are handled consistently.

## 4. Functional Requirements

### 4.1 Sutta headers (HTML path — SuttaCentral)

1. The plain-text conversion (`sutta_html_to_plain_text` in
   `backend/src/helpers.rs`) **must remove the entire `<header>…</header>`
   region except the sutta title line**, for both single-line and **multi-line**
   header blocks. (The current `<header(.*?)</header>` regex fails on multi-line
   headers such as ja239 because it lacks the `(?s)` DOTALL flag.)
2. Within a header, the **division** and **subdivision** lines
   (`<li class='division'>`, `<li class='subdivision'>`, and analogous
   collection/chapter markers) **must be removed** from the plain text.
3. The sutta **title line** (the `<h1>` inside the header, e.g.
   `239. Harita-Mata Jātaka`) **must be preserved** in the plain text —
   **including any leading reference number** (e.g. keep `239 harita-mata
   jātaka`).
4. Example — ja239/en/rouse: `content_plain` must start with the title
   (`239 harita mata jātaka …` per normalization) followed by the body,
   with the leading `stories of the buddha s former births book 2 dukanipāta`
   division/subdivision text removed.

### 4.2 Sutta headers (Bilara JSON path)

5. For segmented (Bilara JSON) suttas, the header segments — the `:0.x` keys
   holding nikāya / vagga / division / subdivision names — **must be excluded**
   from `content_plain`, while the **title segment** (e.g. `Araññasutta`)
   **must be preserved**.
6. **The segment index number is NOT a reliable title identifier** — the title
   segment is `:0.3` in sn1.10/pli/ms but `:0.4` in thag7.3/en/sujato, and
   varies by how many collection levels a text has. Instead, the title is
   identified structurally: the Bilara **template** maps the title segment to an
   `<h1>` element (`"<h1 class='sutta-title'>{}</h1></header>"`), while the
   other `:0.x` segments map to `<ul>`/`<li>` division/subdivision markup.
7. **Preferred approach — reuse the HTML-side rule.** The JSON path already
   renders the segments to HTML via the template and then calls
   `sutta_html_to_plain_text` (see `suttacentral.rs` lines ~606–621). Because
   the template wraps the title in `<h1>` inside `<header>`, the **same**
   HTML-side rule from §4.1 (remove the header region but preserve its `<h1>`)
   correctly handles the JSON path too — no segment-index logic is needed. This
   is verified: **every** HTML `<header>` in the shipped DB (7,811 of them)
   contains an `<h1>`; none would lose its title.
8. Example — sn1.10/pli/ms: `content_plain` must start with `araññasutta
   sāvatthinidānaṁ …`, with `saṁyutta nikāya 1 10 1 naḷavagga` removed.

### 4.3 Sutta footers

9. The plain-text conversion **must remove the `<footer>…</footer>` region**
   (publication credits, license, editor, scanning notes) from
   `content_plain`, for both single-line and multi-line footer blocks. This
   currently does not happen at all.
10. **Both** sutta paths already run `bilara_html_post_process`
    (`helpers.rs`, HTML path at `suttacentral.rs:543`, JSON path at ~618), which
    rewrites `<footer>` → `<footer class='noindex'>` *before*
    `sutta_html_to_plain_text` runs. Verified against the shipped DB: **all 4,450
    footers are `<footer class='noindex'>`; there are zero bare `<footer>`**. The
    removal regex must therefore match `<footer` with optional attributes
    (`<footer\b[^>]*>`), which covers both the bare and `noindex` forms.
11. Scope note — **focus on the known footer type**: the `<footer>` element
    (always `noindex`-marked by bootstrap). All `noindex` markers in the corpus
    are on `<footer>` (0 non-footer `noindex` elements), so footer-element
    removal fully satisfies the `noindex` convention. Bespoke per-source footer
    hunting beyond this is not required for suttas.
12. Example — ja239/en/rouse: the trailing `the jātaka or stories of the …`
    publication text (inside `<footer class='noindex'>`) must not appear in
    `content_plain`.

### 4.4 Dictionary footers (DPD)

The DPD footer boilerplate is made of **three distinct structures**, and these
are **interleaved with real content, not a single trailing block** (verified on
`cūḷā/dpd`: grammar → *feedback* → **example verse** → *feedback* → **declension
table** → *feedback* → *inflection note* → *loading placeholders*). Entry counts
below are over the 170,422 `/dpd` entries in the shipped `dictionaries.sqlite3`;
a single entry typically contains **multiple** feedback elements (3 in `cūḷā`).
All three structures **must be excluded** from `definition_plain`:

13. **Feedback blocks** — `<p class=dpd-footer>…</p>` (87,110 entries, but ~3
    elements each). Wording varies ("Did you spot a mistake?", "Correct it
    here", "Is some word missing from the dictionary?", "Can you think of a
    better example?", "Report it here", "Something missing?") — all inside
    `<p class=dpd-footer>`, so **class-based removal catches every variant** and
    is preferred over text matching. **Two structural gotchas**: (a) the `<p>` is
    **not closed** with `</p>` — it runs to the next block tag; (b) it **contains
    nested `<a>`/`<br>`/`<span>`**. So the element must be matched from
    `<p class=dpd-footer>` up to the next block-level tag
    (`<p`, `<div`, `</div>`, `<table`, `</table>`, `<h…>`, or end), spanning the
    inner inline tags. Remove **all** occurrences per entry.
14. **Loading placeholders** — `<div class="dpd content hidden" id=…>… loading...</div>`
    (89,617 entries; e.g. `family word loading...`, `compound families
    loading...`, `sets loading...`, `frequency loading...`, `feedback
    loading...`). **These must be identified by their `id` prefix, NOT by their
    class.** The class `dpd content hidden` is **also** applied to real-content
    divs — `grammar_<word>`, `example_<word>` (holds an example **verse**,
    verified non-empty), `declension_<word>` — so removing by class would
    wrongly delete legitimate content. The loading placeholders' ids are
    generated from the page word as one of these fixed prefixes followed by the
    (sanitized) word:
    - `family_word_<word>`
    - `family_compound_<word>`
    - `family_set_<word>`
    - `frequency_<word>`
    - `feedback_<word>`

    Remove `<div>…</div>` elements whose `id` begins with one of these five
    prefixes (these divs *are* `</div>`-closed and contain only plain text).
15. **Inflection-not-found note** — a **bare** `<p>Inflections not found in any
    Pāḷi corpus, or …</p>` (78,974 entries). No class or id (verified: always a
    bare `<p>` immediately after `</table>`). Also **unclosed** and contains a
    nested `<span class=gray>grayed out</span>`; match from its **known leading
    text** ("Inflections not found in any Pāḷi corpus", case-insensitive) up to
    the next block-level tag.
16. **NOT a truncate-to-end operation.** Because feedback blocks appear *early*
    and *between* real content sections (the earliest footer marker is a feedback
    block in ~73% of entries, sitting before the example verse and declension
    table), cutting from the first footer marker to the end of the entry would
    **destroy the example verse and declension table**. Each structure must be
    removed **individually and in place**.
17. The **example verse**, **grammar section**, and **declension / inflection
    forms table** (all real content) **must be preserved** in `definition_plain`.
18. Not every DPD entry has all three structures (~53% carry feedback/loading;
    ~46% carry the inflection-not-found note). Removal of each structure must be
    independent, idempotent, and no-op cleanly when a structure is absent.
19. The DPD `<p class=sutta>…</p>` example-source stripping already implemented
    in `dpd_strip_sutta_ref_paragraphs` must continue to work (the example verse
    section contains such `<p class=sutta>` paragraphs); the new footer removal
    is **additive and composed with it**.

### 4.5 Mechanism & centralization

20. Prefer **marker-based removal** using the *most specific reliable* marker per
    structure — a class only when that class uniquely identifies boilerplate
    (`noindex`, semantic `<header>`/`<footer>`, DPD `dpd-footer`), or an **`id`
    prefix** where the class is ambiguous (DPD loading placeholders — see FR 14).
    Do not remove by a class that is shared with legitimate content.
21. Where a source's markup has **no** usable class/id (e.g. the DPD "Inflections
    not found…" bare `<p>`), add a **targeted rule** that matches on the known
    boundary text. The chosen matching strategy per source must be documented
    (see §6).
22. Header/footer removal must be **idempotent**: running the plain-text
    conversion twice (or re-bootstrapping) yields the same result and never
    double-processes.

### 4.6 Delivery

23. After the functions are corrected, **re-bootstrap** the shipped
    `appdata.sqlite3` (suttas) and `dictionaries.sqlite3` (DPD dict_words) so
    the regenerated `content_plain` / `definition_plain` values ship to users.
    No in-place migration of existing DBs is required.
24. The FTS5 / Tantivy indexes derived from these fields must be rebuilt as
    part of the re-bootstrap so search reflects the cleaned text.

### 4.7 Tests

25. Add Rust unit tests in `backend/src/helpers.rs` covering:
    - Multi-line `<header>` removal with the `<h1>` title preserved (ja239 shape).
    - Bilara-rendered header removal with the `<h1 class='sutta-title'>` title
      segment kept (sn1.10 and thag7.3 shapes — differing header-segment counts).
    - `<footer class='noindex'>` removal (multi-line).
    - DPD footer removal for all three structures — **multiple** unclosed
      `<p class=dpd-footer>` (with nested `<a>`/`<br>`), id-prefixed loading
      `<div>`s, and the bare unclosed `<p>Inflections not found…` — asserting the
      **interleaved example verse, grammar, and declension table are preserved**
      and the class-sharing `grammar_`/`example_`/`declension_` divs are *not*
      removed. Include an idempotency assertion (second pass is a no-op).

## 5. Non-Goals (Out of Scope)

- Non-DPD StarDict dictionaries and imported EPUB/PDF/HTML books are **out of
  scope** for this iteration (only SuttaCentral HTML suttas, Bilara JSON
  suttas, and DPD dictionary are covered).
- Changing the **rendered HTML** shown to the user — this PRD only affects the
  plain-text indexing fields, not the displayed `content_html` /
  `content_json` / `definition_html`.
- Changing search ranking/scoring algorithms beyond the effect of cleaner
  indexed text.
- In-place migration of already-shipped user databases (delivery is via
  re-bootstrap).
- Reworking the segment-level `iti` sandhi / punctuation normalization already
  documented in
  `docs/text-processing-for-contains-match-and-fulltext-match-search.md`.

## 6. Technical Considerations

- **Primary code locations:**
  - `backend/src/helpers.rs`: `sutta_html_to_plain_text` (header/footer
    removal), `compact_rich_text`, `compact_plain_text`,
    `bilara_content_footer_replace` (the `<footer class='noindex'>` producer).
  - `cli/src/bootstrap/suttacentral.rs`: HTML path (line ~552) and Bilara JSON
    path (lines ~606–621) that call `sutta_html_to_plain_text`.
  - `backend/src/stardict_parse.rs`: `parse_word` → `compact_rich_text` for
    `definition_plain` (line ~149); pair with the existing
    `dpd_strip_sutta_ref_paragraphs`.
- **Known bug to fix:** `RE_HEADER = Regex::new(r"<header(.*?)</header>")` in
  `sutta_html_to_plain_text` is not DOTALL, so multi-line headers are never
  removed. Use `(?s)` (and add an analogous footer regex).
- **Shared entry point:** `sutta_html_to_plain_text` is called by **6 bootstrap
  sources** (SuttaCentral HTML + JSON, dhammatalks_org, nyanadipa, tipitaka_xml/
  CST, buddha_ujja, dhammapada_munindo). The fix is centralized there and is safe
  for all of them (every source's `<header>` has an `<h1>`; CST has no `<footer>`;
  header-less sources are unaffected).
- **Title preservation (both paths + two markup shapes, one rule):** the title is
  the `<h1>` inside `<header>`; removal keeps the `<h1>` text and drops everything
  else in the header. Two shapes exist: SuttaCentral
  `<header><ul><li class='division'>…</li></ul><h1>…</h1></header>` and **CST**
  `<header><h3>Saṁyuttanikāyo 1.10</h3><h1>10. Araññasuttaṁ</h1></header>` (nikāya
  in `<h3>`). "Keep `<h1>`, drop the rest" handles both. The JSON path renders to
  HTML with an `<h1 class='sutta-title'>` title segment *before* the plain-text
  pass, so the same HTML rule covers it — **do not** key off the `:0.x` segment
  index (it varies: `:0.3` vs `:0.4`). Verified: all 7,811 HTML `<header>` blocks
  contain an `<h1>`, and **all headers are multi-line** (today's non-DOTALL regex
  strips nothing from them).
- **DPD footer detection (three structures, removed in place — NOT truncate-to-
  end).** DPD `<p>` tags are **unclosed** (run to the next block tag) and
  footer/note paragraphs contain nested inline tags; footer elements are
  **interleaved with real content** (example verse, declension table), so each
  structure is removed individually.
  - **Regex-crate constraint (mandatory):** `backend` uses `regex = "1.0"`, which
    has **no lookaround** — a `(?=…)` pattern fails to compile and the `.unwrap()`
    on it **panics**. "Up to the next block tag" is therefore expressed **not** as
    a lookahead, and **not** by consuming+re-inserting the delimiter (that swallows
    the `<p` of an adjacent `<p class=dpd-footer>` and leaves it unmatched), but by
    **enumerating the allowed inner inline tags** so the match halts naturally at
    the first block tag. Concrete patterns:
  - `<p class=dpd-footer>` — remove **all** occurrences (≈3/entry). Class-based
    match covers all wording variants ("Correct it here", "Report it here", "Can
    you think of a better example?", …):
    `(?s)<p class=dpd-footer>(?:[^<]|</?a\b[^>]*>|<br\s*/?>|</?span\b[^>]*>)*`.
  - loading placeholders — remove `<div>…</div>` whose `id` starts with
    `family_word_` / `family_compound_` / `family_set_` / `frequency_` /
    `feedback_`. **Not** by the `dpd content hidden` class, which is shared with
    the real `grammar_` / `example_` / `declension_` divs (`example_<word>` holds
    an example verse — verified non-empty). These divs are `</div>`-closed and
    unnested, so a plain non-greedy body is safe:
    `(?s)<div\b[^>]*\bid=["']?(?:family_word_|family_compound_|family_set_|frequency_|feedback_)[^>]*>.*?</div>`.
  - bare `<p>Inflections not found in any Pāḷi corpus…</p>` — no class; match by
    known leading text (case-insensitive), spanning the nested
    `<span class=gray>`, up to the next block tag:
    `(?is)<p>\s*Inflections not found in any pāḷi corpus(?:[^<]|</?span\b[^>]*>|<br\s*/?>)*`.
  - **Placement:** because `definition_html` must stay intact (PRD §5), do the
    removal in a **DB pass over `definition_plain`** modeled on
    `db/dpd.rs::convert_dpd_example_sutta_links`, recomputing
    `definition_plain = compact_rich_text( dpd_strip_footer(
    dpd_strip_sutta_ref_paragraphs( definition_html ) ) )` — **not** in
    `stardict_parse.rs::parse_word` (which also affects non-DPD dicts and runs
    before the html-modifying passes). Composing with
    `dpd_strip_sutta_ref_paragraphs` is required because that pass keeps the
    `<p class=sutta>` paragraphs in `definition_html` — **but** the pass runs
    *after* `convert_dpd_example_sutta_links` has already turned those paragraphs
    into `<p class="sutta"><a href="ssp://suttas/…">DISPLAY</a>`, and the current
    `dpd_strip_sutta_ref_paragraphs` regex (`[^<]*`) stops at the `<a` and would
    leak `DISPLAY`. **`dpd_strip_sutta_ref_paragraphs` must first be extended** to
    span the optional inner `<a>…</a>`
    (`<p class=(?:"sutta"|sutta)>(?:[^<]|<a\b[^>]*>|</a>|<br\s*/?>)*`) — backward
    compatible with the bare form, so the existing pass is unchanged.
- **Matching-strategy investigation required:** the exact `noindex`/element vs.
  text-boundary strategy must be examined per source; the noindex convention is
  the default but does not apply everywhere (notably DPD). Document the chosen
  strategy per source.
- **Docs to update:**
  - `docs/text-processing-for-contains-match-and-fulltext-match-search.md`
    (the header/footer removal step) — resolve the current `- [ ] bootstrap`
    checkbox.
  - `docs/search-snippet-highlight-pipeline.md` if snippet behavior visibly
    changes.
  - `PROJECT_MAP.md` if function responsibilities move.
- **Re-bootstrap:** follows the standard CLI bootstrap flow in `cli/src/`;
  FTS5 index scripts in `scripts/` are re-run as part of it. Verify against the
  test DB at
  `…/bootstrap-assets-resources/dist/simsapa-ng/app-assets/`.

## 7. Success Metrics

1. `sn1.10/pli/ms` `content_plain` starts with `araññasutta …` (no `saṁyutta
   nikāya … naḷavagga`).
2. `ja239/en/rouse` `content_plain` starts with the title (`239 harita mata
   jātaka …`, number preserved) with no `stories of the buddha s former births
   book 2 dukanipāta` prefix, and no trailing `the jātaka or stories of the …`
   footer text.
3. `cūḷā/dpd` (and other DPD entries) `definition_plain` contains the definition
   and inflected forms but **no** "inflections not found in any pāḷi corpus",
   "report it here", or "…loading…" text.
4. A spot-check across a sample of suttas/dict entries shows no header/footer
   boilerplate at the start or end of the plain-text fields.
5. All new and existing Rust tests pass; re-bootstrapped DBs load and search
   normally.

## 8. Open Questions

*(The four questions from the initial draft are now resolved — see below. Only
minor implementation checks remain.)*

- **Resolved — Title identification:** the segment index is unreliable
  (`:0.3` in sn1.10 vs `:0.4` in thag7.3); identify the title via the `<h1>`
  produced by the template and reuse the HTML-side rule (§4.2 FR 6–7).
- **Resolved — HTML headers without `<h1>`:** none exist (0 of 7,811); the
  "preserve `<h1>`" rule is safe (§4.2 FR 7).
- **Resolved — Footer variants:** scope is the known types — semantic
  `<footer>` and the `noindex` convention (§4.3 FR 11).
- **Resolved — DPD boundary markers:** three structures identified with counts,
  removed in place (not truncate-to-end), interleaved with real content
  (§4.4 FR 13–19).

Remaining checks for implementation:

0. **CST body-embedded collection names (new observation, likely out of scope):**
   CST (`…/pli/cst`) bodies embed nikāya/vagga heading names as *body* text
   (e.g. mn1/pli/cst plain: `…majjhimanikāyo mūlapaṇṇāsapāḷi 1 mūlapariyāyavaggo…`),
   which the header/footer rules do not touch. Decide whether this warrants a
   separate follow-up; it is **not** part of this PRD's header/footer scope.
1. **Suttas with no `<header>`:** ~1,458 HTML suttas (e.g. `thanissaro`,
   `nyanadipa`, `munindo` sources) have no `<header>` at all — confirm they carry
   no separate header boilerplate needing removal (spot-check; likely nothing to
   do).
2. **DPD entries with no `</table>` / no footer (~46%):** confirm the removal
   rules no-op cleanly for deconstructor/root/sandhi entries that lack the full
   page template.
3. **`noindex` reaching the plain-text pass:** verify that suttas actually retain
   `<footer class='noindex'>` markup at the point `sutta_html_to_plain_text`
   runs during bootstrap (vs. the footer wrapper being applied only on the
   display render path).
4. **Bilara no-template fallback bypasses the fix (verify):** the JSON path in
   `suttacentral.rs:623-639` has a fallback branch (`tmpl_json == None`) that builds
   `content_plain` by joining **all** segment values — including the `:0.x` header
   segments — and **does not** call `sutta_html_to_plain_text`. The template path
   is assumed to cover every shipped Bilara sutta, but this is unverified. Confirm
   via the bootstrap log: the branch logs `No template available for …`; expect
   **zero** such warnings. If any occur, those suttas' header segments are not
   covered by this PRD (no `<h1>` is produced without a template) and warrant a
   separate follow-up.
