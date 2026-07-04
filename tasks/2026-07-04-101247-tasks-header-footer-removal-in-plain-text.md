# Tasks: Header & Footer Removal for Plain-Text Indexing (Bootstrap)

PRD: `2026-07-04-101247-prd---header-footer-removal-in-plain-text.md`

## Relevant Files

- `backend/src/helpers.rs` — Home of the plain-text pipeline. `sutta_html_to_plain_text`
  (line ~1887) is the single choke point for both sutta paths; `compact_rich_text`
  / `compact_plain_text` / `strip_html` are the downstream steps.
  `dpd_strip_sutta_ref_paragraphs` (line ~620) is the existing DPD strip to mirror
  for the new footer helper. `bilara_html_post_process` (line ~1995) adds
  `class='noindex'` to `<footer>` on the JSON path only. All new unit tests live
  in the `#[cfg(test)]` module here (tests start ~line 2800).
- `cli/src/bootstrap/suttacentral.rs` — Sutta bootstrap. Calls
  `sutta_html_to_plain_text` on the HTML path (line ~552) and, after rendering
  segments to HTML, on the Bilara JSON path (line ~621). No logic change expected;
  used to verify both paths route through the fixed function.
- `backend/src/stardict_parse.rs` — `parse_word` (line ~125) generates the initial
  DPD `definition_plain` via `compact_rich_text` on the `'h'` segment.
- `backend/src/db/dpd.rs` — DPD post-import DB passes. `convert_dpd_example_sutta_links`
  (line ~808) is the model: iterate DPD `dict_words`, recompute `definition_plain`
  from stripped HTML, update in batches before FTS5 indexes exist. The new footer
  pass goes here.
- `cli/src/bootstrap/dpd.rs` — `dpd_bootstrap` (line ~10) orchestrates the DPD
  import + post-passes; the new footer pass is wired in here before
  `create_dictionaries_fts5_indexes`.
- `docs/text-processing-for-contains-match-and-fulltext-match-search.md` — The doc
  to update (resolve the `- [ ] bootstrap` checkbox; describe header/footer rules).
- `PROJECT_MAP.md` — Update if function responsibilities shift.
- `scripts/*-fts5-indexes.sql` — FTS5 index scripts re-run during re-bootstrap (no
  edits; listed because the re-bootstrap depends on them).
- Test DB / SIMSAPA_DIR:
  `…/bootstrap-assets-resources/dist/simsapa-ng/app-assets/{appdata,dictionaries}.sqlite3`
  — used to verify success metrics (sn1.10, ja239, cūḷā).

### Notes

- Run backend tests with `cd backend && cargo test`, or a single test with
  `cd backend && cargo test test_name`. Per project convention, only run tests
  after all sub-tasks of a top-level task are complete; documentation-only changes
  need no test run.
- Build with `make build -B`.
- Header/footer removal must be **idempotent** (re-running the pass yields the same
  output) so re-bootstrap is safe.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, check it off by changing `- [ ]` to
`- [x]`. Update the file after each sub-task, not just after a parent task.

## Tasks

### Specs & dependencies for 1.0

- **Target function:** `sutta_html_to_plain_text(html: &str) -> String` in
  `backend/src/helpers.rs`. It is the shared entry point for **all sutta bootstrap
  sources** — SuttaCentral (`suttacentral.rs`, both HTML and JSON paths) **plus**
  `dhammatalks_org.rs`, `nyanadipa.rs`, `tipitaka_xml.rs` (CST), `buddha_ujja.rs`,
  and `dhammapada_munindo.rs`. Fixing it here satisfies FR 1–8 without touching
  those callers. **The change is safe for all of them** (verified below).
- **Two header markup shapes, one rule:** SuttaCentral uses
  `<header><ul><li class='division'>…</li>…</ul><h1>…</h1></header>`; **CST**
  (tipitaka_xml) uses `<header><h3>Saṁyuttanikāyo 1.10</h3><h1>10. Araññasuttaṁ</h1></header>`
  (nikāya in `<h3>`, title in `<h1>`). "Keep the `<h1>`, drop everything else in
  the header" handles both. Verified across the whole corpus: **every** source's
  `<header>` contains an `<h1>` (0 headers lack one), and **all headers are
  multi-line** (so the current non-DOTALL regex currently strips *nothing* from
  them — header text leaks today). CST has **no `<footer>`**, so the footer rule
  is a no-op there.
- **Header rule (FR 1–7):** Remove the whole `<header>…</header>` region **but
  preserve the `<h1>` inside it** (title, including any leading number). Must be
  **multi-line safe** — the current `<header(.*?)</header>` regex lacks `(?s)`
  DOTALL and silently fails on multi-line headers (ja239). Verified: all 7,811 HTML
  `<header>` blocks contain an `<h1>`, and the Bilara template renders the title as
  `<h1 class='sutta-title'>…</h1></header>`, so one HTML rule covers both paths;
  do **not** key off the `:0.x` segment index (unreliable: `:0.3` vs `:0.4`).
- **Footer rule (FR 9–12):** Remove `<footer>…</footer>` (multi-line safe).
  Verified: **both** sutta paths run `bilara_html_post_process` before the
  plain-text pass (HTML path `suttacentral.rs:543`, JSON path ~618), so the
  footer is **always** `<footer class='noindex'>` — there are **zero bare
  `<footer>`** in the corpus (all 4,450 footers are `noindex`). Match
  `<footer\b[^>]*>…</footer>` (covers both forms). All `noindex` markers are on
  `<footer>` (0 non-footer), so this satisfies the `noindex` convention.
- **Ordering:** header/footer removal happens on the raw HTML, before
  `compact_rich_text` strips remaining tags.

- [x] 1.0 Fix and centralize sutta header/footer removal in the plain-text pass
  - [x] 1.1 In `sutta_html_to_plain_text`, replace the `RE_HEADER` regex with a
        multi-line-safe (`(?s)`) match of `<header\b[^>]*>…</header>` whose
        replacement **preserves the inner `<h1>…</h1>`** (keep the h1 element/text,
        drop the surrounding `<ul>`/`<li>` division/subdivision markup and header
        tags). If no `<h1>` is present, the whole header is removed.
  - [x] 1.2 Add a multi-line-safe footer removal (`(?s)<footer\b[^>]*>.*?</footer>`
        → empty) matching `<footer class='noindex'>` (and any bare `<footer>` for
        robustness). Apply before `compact_rich_text`.
  - [x] 1.3 Confirm ordering: header + footer removal run on the raw HTML string
        first, then the existing `compact_rich_text(&s)` call; verify the `<h1>`
        text survives `compact_rich_text` (word boundaries around tags).
  - [x] 1.4 Add a unit test for the **HTML path** (ja239 shape): a multi-line
        `<header>` with `<ul><li class='division'>…</li><li class='subdivision'>…</li></ul>`
        + `<h1>239. Harita-Mata Jātaka</h1>`, and a trailing `<footer>…</footer>`.
        Assert the title (incl. `239`) is present and the division/subdivision and
        footer text are absent.
  - [x] 1.5 Add a unit test for the **Bilara JSON path** shape: render an
        `<h1 class='sutta-title'>` title with preceding `<ul><li class='division'>`
        header segments and a `<footer class='noindex'>` (sn1.10 and a thag7.3-style
        case where the title is a later segment). Assert the title survives, nikāya/
        vagga are removed, and the noindex footer is removed.
  - [x] 1.5a Add a unit test for the **CST header shape**
        (`<header><h3>Saṁyuttanikāyo 1.10</h3><h1>10. Araññasuttaṁ</h1></header>`):
        assert the `<h1>` title survives and the `<h3>` nikāya line is removed
        (different markup from the SuttaCentral `<ul><li>` shape, same rule).
  - [x] 1.6 Add a regression test asserting idempotency: running
        `sutta_html_to_plain_text` twice yields the same result.
  - [x] 1.7 Run `cd backend && cargo test` and `make build -B`; confirm the new and
        existing tests pass.

### Specs & dependencies for 2.0

- **Where plain is authoritative:** DPD `definition_plain` is first set in
  `stardict_parse.rs::parse_word`, then **recomputed** by
  `convert_dpd_example_sutta_links` / `convert_dpd_epd_word_links` (only for rows
  matching `class=sutta` / `class=epd`). Rows matching neither keep the
  parse-time plain (footers intact). Therefore the footer pass must cover **every
  footer-bearing row** regardless of whether the earlier passes touched it — hence
  the pre-filter is on the footer markers themselves (not on `class=sutta`/`epd`).
  Running last, it is the authoritative final writer of `definition_plain` for
  those rows.
- **HTML is out of scope (PRD §5):** only `definition_plain` changes;
  `definition_html` (the rendered page, incl. the feedback link) stays intact.
- **Composition gotcha:** `convert_dpd_example_sutta_links` keeps the
  `<p class=sutta>` paragraphs in `definition_html` (as ssp links) but strips them
  from plain. If the footer pass recomputes plain from `definition_html`, it must
  **also** apply `dpd_strip_sutta_ref_paragraphs` so those sutta names don't leak
  back in. So the authoritative recompute is
  `compact_rich_text( dpd_strip_footer( dpd_strip_sutta_ref_paragraphs( definition_html ) ) )`.
- **Composition gotcha #2 — `dpd_strip_sutta_ref_paragraphs` is currently blind to
  the CONVERTED paragraph form (must be fixed).** The footer pass runs **after**
  `convert_dpd_example_sutta_links`, which has already rewritten each
  `<p class=sutta>TH155 name…` into `<p class="sutta"><a href="ssp://suttas/…"
  class="sutta-link">DISPLAY</a>`. But the existing helper matches
  `<p class=(?:"sutta"|sutta)>[^<]*` — against the converted markup `[^<]*` stops
  at the very first `<` (the `<a`), so it strips only the opening `<p>` and
  **leaves `<a>DISPLAY</a>`**, whose display text then leaks back into
  `definition_plain` (the exact leak the helper exists to prevent — see
  `helpers.rs:611-615`). The reason the existing pass is unaffected is that it
  feeds `dpd_strip_sutta_ref_paragraphs` the **pre-conversion** `row.definition_html`
  (bare text). Since the footer pass feeds it **post-conversion** HTML, the helper
  **must be extended** to consume an optional inner `<a>…</a>`/`<br>` before the next
  block tag, matching *both* forms. Extended pattern (regex crate has **no
  lookahead** — enumerate the allowed inner inline tags instead of a lookahead
  boundary):
  `<p class=(?:"sutta"|sutta)>(?:[^<]|<a\b[^>]*>|</a>|<br\s*/?>)*`
  — this reduces to the old `[^<]*` behavior on the bare form (so the existing
  pass is unchanged) and now also spans the ssp `<a>` wrapper. It stops at the next
  block tag (`<p`/`<div`/`<table`/`<h…>`) because those are not in the allowed set.
- **Three footer structures (FR 13–19) — removed IN PLACE, never truncate-to-end.**
  Footer elements are **interleaved with real content** (verified on `cūḷā`:
  grammar → feedback → **example verse** → feedback → **declension table** →
  feedback → inflection note → loading divs). DPD `<p>` tags are **unclosed**
  (run to the next block tag) and contain nested inline tags.
- **⚠️ Regex-crate constraint (load-bearing):** `backend` uses `regex = "1.0"`,
  which has **NO lookahead/lookaround** (`(?=…)` fails to compile → `.unwrap()`
  **panics**). So "match up to the next block tag" **must not** be written as a
  lookahead, and it **must not** be written by *consuming* the delimiter and
  re-inserting it (`…(<p|<div|…)` → `$1`) either — consuming the delimiter would
  swallow the `<p` of an immediately-following `<p class=dpd-footer>` and leave its
  successor unmatched (adjacent feedback prompts are common, ≈3/entry). Instead,
  **enumerate the allowed inner inline tags** so the match naturally halts at the
  first block tag without consuming it (same technique as the extended
  `dpd_strip_sutta_ref_paragraphs` above). Concrete patterns given per structure
  below.
  1. `<p class=dpd-footer>` — feedback prompts, **≈3 per entry**, varied wording
     (all caught by the class). Remove **all** occurrences, each matched from the
     opening tag through its inner `<a>`/`<br>`/`<span>` up to (but **not**
     including) the next block tag:
     `(?s)<p class=dpd-footer>(?:[^<]|</?a\b[^>]*>|<br\s*/?>|</?span\b[^>]*>)*`
     → replace with `""`. (No lookahead; halts at `<p`/`<div`/`<table`/`<h…>`
     because those aren't in the allowed inner-tag set. `replace_all` then handles
     consecutive prompts correctly since the delimiter is left intact.)
  2. `<div …id=…>…loading...</div>` whose `id` starts with `family_word_`,
     `family_compound_`, `family_set_`, `frequency_`, or `feedback_` — remove by
     **id prefix, NOT class** (`dpd content hidden` is shared with the real
     `grammar_`/`example_`/`declension_` divs; `example_<word>` holds a verse).
     These divs **are** `</div>`-closed and hold only plain text (no nested
     `<div>`), so a plain non-greedy body is safe:
     `(?s)<div\b[^>]*\bid=["']?(?:family_word_|family_compound_|family_set_|frequency_|feedback_)[^>]*>.*?</div>`
     → replace with `""`. (`[^>]*\bid=` tolerates other attributes before `id`;
     the id value may be quoted or unquoted.)
  3. bare unclosed `<p>Inflections not found in any Pāḷi corpus…</p>` — no class;
     match by known leading text (case-insensitive), spanning its nested
     `<span class=gray>`, up to the next block tag:
     `(?is)<p>\s*Inflections not found in any pāḷi corpus(?:[^<]|</?span\b[^>]*>|<br\s*/?>)*`
     → replace with `""`. (`(?i)` handles the case-insensitive leading text; the
     same enumerate-inner-tags trick halts at the following `<div …loading…>`.)
- **Dependencies:** mirror `convert_dpd_example_sutta_links` batching (id > ?
  keyset, run before `create_dictionaries_fts5_indexes` so FTS sync triggers
  don't fire). Reuse `compact_rich_text`. Only touch footer-bearing rows via a
  `LIKE` pre-filter (like the existing passes filter on `class=sutta`/`class=epd`).

- [x] 2.0 Add DPD dictionary footer removal for `definition_plain`
  - [x] 2.0a **Extend `dpd_strip_sutta_ref_paragraphs`** (`helpers.rs:620`) to also
        strip the **converted** `<p class="sutta"><a href="ssp://suttas/…">DISPLAY
        </a>` form, not just the bare pre-conversion text. Replace the current
        `<p class=(?:"sutta"|sutta)>[^<]*` with
        `<p class=(?:"sutta"|sutta)>(?:[^<]|<a\b[^>]*>|</a>|<br\s*/?>)*`. This is
        backward-compatible (reduces to `[^<]*` on the bare form, so
        `convert_dpd_example_sutta_links`'s existing use is unchanged) and is
        **required** so the footer pass — which runs on post-conversion HTML — does
        not leak the sutta display text back into `definition_plain`. Add/extend a
        unit test asserting both the bare and the `<a>`-wrapped forms are stripped.
  - [x] 2.1 Add a `dpd_strip_footer(html: &str) -> String` helper in
        `backend/src/helpers.rs` (next to `dpd_strip_sutta_ref_paragraphs`) that
        removes, **in place**, using the **no-lookahead, enumerate-inner-tags**
        patterns from the 2.0 spec (the `regex` crate has no lookaround; do **not**
        consume-and-reinsert the delimiter — it breaks adjacent footers):
        (a) **all** `<p class=dpd-footer>` elements
        (`(?s)<p class=dpd-footer>(?:[^<]|</?a\b[^>]*>|<br\s*/?>|</?span\b[^>]*>)*`);
        (b) `<div…>…</div>` whose `id` begins with `family_word_` /
        `family_compound_` / `family_set_` / `frequency_` / `feedback_`
        (`(?s)<div\b[^>]*\bid=["']?(?:family_word_|family_compound_|family_set_|frequency_|feedback_)[^>]*>.*?</div>`);
        and (c) the bare `<p>Inflections not found in any Pāḷi corpus…</p>` note
        (`(?is)<p>\s*Inflections not found in any pāḷi corpus(?:[^<]|</?span\b[^>]*>|<br\s*/?>)*`).
        Idempotent; no-op when a structure is absent. **Must not** touch
        `grammar_`/`example_`/`declension_` divs, the declension `<table>`, or
        `<p class=sutta>`.
  - [x] 2.2 Add unit tests for `dpd_strip_footer` using a realistic interleaved
        fixture (a cūḷā-shaped input: grammar table, an `example_` div with a
        verse + `<p class=sutta>`, multiple `<p class=dpd-footer>` prompts with
        nested `<a>`, a declension `<table>`, the bare `Inflections not found`
        note, and the `family_*`/`frequency_`/`feedback_` loading divs). Assert:
        all feedback prompts + loading placeholders + inflection note are gone;
        the **example verse, grammar, and declension table are preserved**; the
        `grammar_`/`example_`/`declension_` divs survive; a second application is
        a no-op. **Also** include a fixture whose `example_` div contains a
        **converted** `<p class="sutta"><a href="ssp://suttas/…">DISPLAY</a>`
        paragraph and assert (via the full recompute pipeline
        `compact_rich_text(dpd_strip_footer(dpd_strip_sutta_ref_paragraphs(html)))`)
        that `DISPLAY` does **not** appear in the resulting plain text (guards the
        Composition-gotcha-#2 regression).
  - [x] 2.3 Add a `strip_dpd_footers_from_plain(dict_db_path: &Path) -> Result<()>`
        pass in `backend/src/db/dpd.rs`, modeled on
        `convert_dpd_example_sutta_links`: batch-iterate (keyset `id > ?`) the
        `dict_label = 'dpd'` rows whose `definition_html LIKE '%dpd-footer%' OR
        LIKE '%loading...%' OR LIKE '%Inflections not found%'`, recompute
        `definition_plain = compact_rich_text( dpd_strip_footer( dpd_strip_sutta_ref_paragraphs( definition_html ) ) )`,
        and `UPDATE dict_words SET definition_plain = ?` **only** (leave
        `definition_html` unchanged). Skip the write when unchanged.
  - [x] 2.4 Wire `strip_dpd_footers_from_plain` into `cli/src/bootstrap/dpd.rs`
        `dpd_bootstrap`, after `convert_dpd_epd_word_links` and **before**
        `create_dictionaries_fts5_indexes` (so triggers don't fire and it operates
        on the final, epd/sutta-converted `definition_html`).
  - [x] 2.5 Run `cd backend && cargo test` and `make build -B`; confirm tests pass.

### Specs & dependencies for 3.0

- **Delivery is re-bootstrap (PRD §4.6):** regenerate the shipped DBs so the new
  `content_plain` / `definition_plain` ship; FTS5 (scripts in `scripts/`) and
  Tantivy indexes are rebuilt as part of the bootstrap flow.
- **Success metrics (PRD §7) to verify** against the test DB:
  sn1.10/pli/ms, ja239/en/rouse, cūḷā/dpd.
- **Remaining checks (PRD §8):** headerless suttas, footerless/tableless DPD rows
  (no-op cleanly), and confirming `noindex`/`<footer>` markup actually reaches
  `sutta_html_to_plain_text` at bootstrap time.
- **Bilara no-template fallback (gap — verify):** the JSON path in
  `suttacentral.rs:623-639` has a **fallback branch** (`tmpl_json == None`) that
  builds `content_plain` by `segments.values().join("\n\n")` and **does not** call
  `sutta_html_to_plain_text` — so the `:0.x` nikāya/vagga/division header segments
  would leak there, bypassing the whole fix. The template path is assumed to cover
  every shipped Bilara sutta; this must be **confirmed**, not assumed. The branch
  emits a `logger::warn("No template available for …")`, so the bootstrap log is
  the check: zero such warnings ⇒ no leak. If any occur, they are out of the
  header/footer rule's reach (no `<h1>` is produced without a template) and need a
  separate decision.

- [ ] 3.0 Re-bootstrap the databases and verify
  - [ ] 3.1 Run the CLI bootstrap to regenerate `appdata.sqlite3` (suttas) and
        `dictionaries.sqlite3` (DPD), including the FTS5 index scripts and Tantivy
        index rebuild. Capture/verify a clean run (no errors/warnings for the
        affected paths).
  - [ ] 3.2 Verify sn1.10/pli/ms `content_plain` starts with `araññasutta
        sāvatthinidānaṁ …` (no `saṁyutta nikāya … naḷavagga`).
  - [ ] 3.3 Verify ja239/en/rouse `content_plain` starts with the title (incl.
        `239`, no `stories of the buddha s former births book 2 dukanipāta` prefix)
        and has no trailing `the jātaka or stories of the …` footer text.
  - [ ] 3.4 Verify cūḷā/dpd (and a few other DPD entries) `definition_plain`
        contains the definition + inflected forms but **no** "inflections not
        found…", "report it here", or "…loading…" text.
  - [ ] 3.5 Spot-check the PRD §8 remaining cases: a headerless HTML sutta
        (e.g. `thanissaro`/`nyanadipa` source), a **CST** sutta (`…/pli/cst` —
        `<h3>` nikāya removed, `<h1>` title kept), a DPD deconstructor/root entry
        lacking `</table>`/footer (confirm no content loss), and confirm the
        `<footer class='noindex'>` markup is present at the point
        `sutta_html_to_plain_text` runs.
  - [ ] 3.5a Confirm the **Bilara no-template fallback** (`suttacentral.rs:623-639`)
        is not exercised by any shipped sutta: grep the bootstrap log for
        `No template available for` — expect **zero** hits. If any appear, record
        the uids (their header segments are not covered by this PRD) and flag for a
        follow-up decision.
  - [ ] 3.5b **Observation (out of scope — record, do not fix here):** CST bodies
        also embed nikāya/vagga/vagga-heading names as *body* text (e.g. mn1/pli/cst
        `content_plain`: `…majjhimanikāyo mūlapaṇṇāsapāḷi 1 mūlapariyāyavaggo…`),
        which the header rule does not touch. If this proves to pollute search,
        it is a separate follow-up (not part of this PRD's header/footer scope).
  - [ ] 3.6 Sanity-check a search for a title word (e.g. `araññasutta`) still
        returns the sutta, and a common collection word no longer matches purely
        on header text.

- [ ] 4.0 Update documentation
  - [ ] 4.1 Update
        `docs/text-processing-for-contains-match-and-fulltext-match-search.md`:
        describe the header rule (remove header, preserve `<h1>` title incl.
        number), the footer rule (`<footer>` / `noindex`), and the DPD footer rule
        (three structures, id-prefix vs class vs text-boundary); resolve the
        `- [ ] bootstrap` checkbox.
  - [ ] 4.2 Update `PROJECT_MAP.md` if function responsibilities/locations changed
        (new `dpd_strip_footer`, `strip_dpd_footers_from_plain`).
  - [ ] 4.3 Add a short cross-reference in `AGENTS.md` (the CLAUDE.md symlink
        target) notable-feature-docs list only if a new standalone doc is warranted;
        otherwise leave the existing bullet and skip.
