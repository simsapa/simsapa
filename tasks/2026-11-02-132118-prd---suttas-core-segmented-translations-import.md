# PRD: Import segmented translations from suttas-core

## 1. Introduction / Overview

Many English sutta translations in Simsapa are stored only as **HTML pages**
(`suttas.content_html`), with no Bilara-style segmented text
(`suttas.content_json`). An HTML-only record cannot be lined up with the Pāli
in the multi-column reading view, cannot be served by `/sutta.json` or
`/sutta_bilara.json`, and is aligned with nothing.

The [suttas-core](https://git.unxr.net/adicco/suttas-core/) project
(cloned at `bootstrap-assets-resources/suttas-core`) produces a SQLite
database with **segmented** versions of these translations, keyed by the
SuttaCentral segment ids of the Pāli root text (`ms`). Its output for the
bootstrap is at:

```
bootstrap-assets-resources/suttas-core-db/current/suttas.sqlite
```

(`current` is a symlink to the dated folder to use, e.g. `2026-10-31`.)

This feature adds a bootstrap step that takes segmented translations from
suttas-core and uses them **instead of** the HTML records Simsapa would
otherwise ship, under these rules:

- **Every translator except Ṭhānissaro**: a suttas-core text replaces the
  matching Simsapa record only when that record is **HTML-only**. A record
  that already has `content_json` (SuttaCentral's Bilara text: Sujato,
  Brahmali, …) is kept as it is, and a sutta Simsapa would not have is **not**
  added. The decision is made from the data, never from translator names. In
  practice this replaces Bhikkhu Bodhi's and the PTS translators' HTML
  records.
- **Ajahn Ṭhānissaro** (suttas-core `than`, Simsapa `thanissaro`): import
  **every** segmented text from suttas-core. These replace the HTML records
  imported from the dhammatalks.org mirror, and take **SuttaCentral
  numbering** (see §7.2 — this changes uids).

Translator footnotes from suttas-core are imported into `sutta_comments`, and
the reader shows them in the existing bottom footnote bar as they scroll into
view, as it does today for Ṭhānissaro's HTML pages.

A separate bootstrap command surveys the differences between the
dhammatalks.org import and suttas-core and writes a report, so fixes can later
be ported back to suttas-core.

**Goal:** ship segmented, Pāli-aligned text for Bodhi, PTS and Ṭhānissaro
translations without adding any Bodhi/PTS sutta Simsapa does not already
carry, and without losing footnotes.

## 2. Goals

1. Every Bodhi and PTS-translator HTML record that has a segmented
   counterpart in suttas-core, and no `content_json` of its own, is replaced
   by it (about 1,165 Bodhi and 144 PTS
   records with the 2026-10-31 data; see §8).
2. Every Ṭhānissaro text segmented in suttas-core (1,443 with the 2026-10-31
   data) is in Simsapa as a segmented record under its SuttaCentral uid.
3. No Ṭhānissaro sutta appears twice (once under the old dhammatalks.org
   numbering and once under the SuttaCentral numbering).
4. Ṭhānissaro texts with no segmented version (the Khuddakapāṭha, `kp1`–`kp9`)
   keep their dhammatalks.org HTML record.
5. Footnotes of the imported records, SuttaCentral's comments, and the HTML
   pages and library books that already have footnotes are shown in the
   reader's bottom bar while their marker is in view; the note text there can
   be selected and copied, its sutta links work, and the bar respects the
   "show footnotes bar" setting.
6. A survey command produces a report of differences, for porting fixes back
   to suttas-core.

## 3. User Stories

- *As a reader*, I open MN 10 by Bhikkhu Bodhi next to the Pāli in the
  multi-column view and the two line up segment by segment, as Bhikkhu
  Sujato's translation does today.
- *As a reader of Ajahn Ṭhānissaro's translations*, I open AN 10.176 and get
  his translation of the Cunda sutta, the same sutta as every other AN 10.176
  translation — not a different sutta numbered by the dhammatalks.org files.
- *As a reader*, when I scroll a sutta with Ṭhānissaro's notes, each note
  appears in the bar at the bottom of the reader when its marker comes into
  view, and clicking the marker opens it in the pop-up.
- *As a reader of Bhikkhu Sujato's translations*, his notes appear in the
  bar at the bottom of the reader as I scroll, instead of behind a `✱` I
  have to click.
- *As a reader*, I can select a sentence of a note in the footnote bar and
  copy it, and a sutta reference in the note opens that sutta.
- *As a reader who prefers not to have the bar*, I turn off "show footnotes
  bar" and notes open only when I click their number.
- *As an agent using the localhost API*, I can fetch `/sutta.json` for
  `mn10/en/bodhi` and `an10.176/en/thanissaro` and get segmented text.
- *As the maintainer*, I can run one command to get a report of where the
  dhammatalks.org import and suttas-core disagree, to send fixes upstream.

## 4. Functional Requirements

### 4.1 Locating the suttas-core database

1. The bootstrap must read suttas-core from
   `<bootstrap_assets_dir>/suttas-core-db/current/suttas.sqlite`, following
   the symlink.
2. If the file does not exist, the bootstrap must log a warning and skip the
   suttas-core step (as the other optional importers do). Records stay as the
   earlier importers wrote them.
3. The bootstrap must log the resolved path (the symlink target, i.e. the
   dated folder) so the bootstrap log records which data was used.
4. The suttas-core database must be opened **read-only**.

### 4.2 Position in the bootstrap

5. The suttas-core step must run in `bootstrap()` in
   `bootstrap_cli/src/bootstrap/mod.rs` **after** the SuttaCentral (`en`,
   `pli`) import and the dhammatalks.org import, and **before** the
   navigation-tree builds. The navigation tree resolves each leaf's
   `sutta_uid` against rows that exist, so it must see the final uids.
6. The step must be a new importer implementing `SuttaImporter`, in a new
   file `bootstrap_cli/src/bootstrap/suttas_core.rs`.
7. The step must honour the bootstrap's `--limit` option in the same way the
   other importers do.

### 4.3 Building a segmented record from suttas-core

8. For a given suttas-core `(sutta_id, translator, language)`, the importer
   must build:
   - `content_json`: a JSON object of `segment_id → content` with **one key
     per key of the template, in template order**, the value taken from
     `sutta_segments` or `""` when the translation has no such segment. Key
     order is the reading order: never sort the keys; build it with an
     order-preserving map such as `IndexMap`. The renderer walks the
     *translation's* keys and wraps each in its template entry, so a missing
     key would drop that entry's markup (`<section>`, `<h2>`, a closing
     `</p>`). Bodhi, PTS and Walshe cover every Pāli segment, but 14
     Ṭhānissaro texts do not (964 segments, 752 of them carrying markup; he
     translated only parts of `dn21` and `dn26`, and some texts lack heading
     segments). Every translation segment id is also a Pāli segment id, so
     filling from the template loses nothing.
   - `content_json_tmpl`: the template of Simsapa's own `<sutta_id>/pli/ms`
     record, which is the same SuttaCentral template the Sujato records use.
     If that record does not exist, the importer must build the template from
     suttas-core's `segment_structure.template` (same `segment_id → template`
     shape, in `ord` order) and log that it did so.
   - `content_plain`: the plain text of the segments, produced with the same
     helper the SuttaCentral importer uses for Bilara records, so search
     behaves the same as for Sujato's translations.
   - `content_html`: `NULL`.
9. The importer must apply the same text normalisation as the other importers,
   at least `consistent_niggahita()` (269 Bodhi/PTS/Ṭhānissaro segments in the
   2026-10-31 data contain `ṃ`).
10. `copyright` and `license` must be set from `sutta_copyrights`
    (`copyright_holder` → `copyright`, `copyright_type` → `license`).
11. Segments with empty content must be kept (as SuttaCentral's are); the
    template decides what is rendered.
11a. `source_info` must be set to `suttas-core <dated folder>` (e.g.
    `suttas-core 2026-10-31`) on every record the step writes, so the data's
    origin is visible in the database. The column is unused today (`NULL` in
    every row).

### 4.4 Every other translator: replace only HTML records Simsapa already has

The import decision is made from the **data**, not from translator names: a
suttas-core translation is used only where it turns an HTML-only Simsapa
record into a segmented one. No translator is listed as included or
excluded. A translator suttas-core adds later is handled by the same rule.

12. **Matching.** For each suttas-core `(sutta_id, translator)` with
    segments, other than `than` (§4.5), the importer looks for Simsapa
    records with uid `<sutta_id>/<language>/<source_uid>`, where
    `source_uid` comes from a **label map**:
    - by default the suttas-core label itself (`bodhi` → `bodhi`,
      `sujato` → `sujato`, `walshe` → `walshe`, any new label → itself);
    - `pts` → each of `horner`, `woodward`, `hare`, `tw_rhysdavids`,
      `tw-caf_rhysdavids`, `caf_rhysdavids`, `rhysdavids-brasington`
      (suttas-core keeps one label for the Pali Text Society translations;
      Simsapa keeps SuttaCentral's per-translator labels);
    - `than` → `thanissaro` (used by §4.5).

    The map holds only the labels that differ, as one named constant. It is
    used **only to find the matching record**. It never decides whether to
    import: a label missing from the map falls back to the identity, and a
    wrong or missing entry can only cause a missed replacement, never an
    unwanted import.
13. **Decision**, per matched Simsapa record:
    - **HTML-only** (`content_json` empty) → update the record in place: set
      the fields of FR 8–10, and keep its `id`, `uid`, `sutta_ref`,
      `nikaya`, `title`, `title_pali`, range fields and `source_uid` (a PTS
      record stays e.g. `mn1/en/horner`). Import its footnotes (§4.7).
    - **Already has `content_json`** (e.g. SuttaCentral's Bilara text) → skip
      it. Do not touch it and do not write suttas-core footnotes for it.
      SuttaCentral's text and comments stay as today.
    - **No matching record** → import nothing (Simsapa does not gain Bodhi,
      PTS or other suttas it would not otherwise carry).
14. A Simsapa HTML record with no suttas-core counterpart (Bodhi `dn1`,
    `dn2`, `dn15` in the 2026-10-31 data) is left as it is.
15. The Pāli root (`ms`, language `pli`) goes through the same rule: Simsapa
    already has every `ms` text with `content_json`, so nothing is written.
16. Expected outcome with the 2026-10-31 data, as a check that the rule
    behaves (these counts are **not** encoded anywhere):

    | suttas-core label | suttas | Simsapa HTML-only → replaced | Simsapa has `content_json` → skipped | not in Simsapa → skipped |
    |---|---|---|---|---|
    | `bodhi` | 3,452 | 1,165 | 0 | 2,287 |
    | `pts` | 3,246 | 144 | 0 | the rest |
    | `walshe` | 34 | 0 | 0 | 34 |
    | `sujato` | 4,050 | 0 | 4,045 | 5 |
    | `brahmali` | 965 | 0 | 419 | 546 |
    | `soma` | 73 | 0 | 73 | 0 |
    | `suddhaso` | 30 | 0 | 30 | 0 |
    | `kovilo` | 1 | 0 | 1 | 0 |

    The Brahmali "not in Simsapa" rows are Vinaya ids that suttas-core keys
    at rule level (e.g. `pli-tv-bi-pm-as1`). The 5 Sujato ones (`an3.41`,
    `an4.273`, `dn14`, `sn35.7`, `sn46.67`) are missing only from the dist
    database this was measured on: the SuttaCentral import produces them,
    and a backend test deleted them afterwards. After a fresh bootstrap they
    exist with `content_json`, so the rule skips them.
16a. **The default English must not change as a side effect.** With no
    columns requested, a Pāli text opens beside "the first segmented
    English" (`default_english_for_pali()` in `backend/src/app_data.rs`),
    chosen in **uid order**. Once Bodhi and the PTS translators are
    segmented, `mn10/en/bodhi` and `mn1/en/horner` sort before
    `…/en/sujato`, so the default would silently switch from Sujato to Bodhi
    or Horner. The selection must keep today's result, with Sujato beside
    the Pāli: prefer a segmented English that is **not** from suttas-core
    (`source_info`, FR 11a), then a suttas-core one, then any other English.
17. The step must log, per suttas-core label: replaced, skipped because
    already segmented, skipped because not in Simsapa, with the uid lists at
    debug level.

### 4.5 Ṭhānissaro: import everything, under SuttaCentral numbering

18. For every suttas-core `sutta_id` with `than` segments, the importer must
    produce a record with uid `<sutta_id>/en/thanissaro` and
    `source_uid = "thanissaro"` (Simsapa's existing label is kept; suttas-core
    calls him `than`).
18a. If a record with that uid already exists **with** non-empty
    `content_json` (e.g. from SuttaCentral), it is kept and the suttas-core
    record is not written (the same decision as FR 13). Any dhammatalks.org record that maps to
    that uid is still deleted, so the sutta is not listed twice.
19. Its `title` must come from `raw_translations.title` of the raw that
    supplies the segments; `title_pali` from Simsapa's `<sutta_id>/pli/ms`
    record; `sutta_ref`, `nikaya` and range fields computed from the uid the
    same way the other importers do (`uid_to_ref`, `uid_to_nikaya`,
    `parse_range_from_uid`).
20. **Old-to-new uid map.** The importer must build a map from each
    dhammatalks.org-derived uid (the uids the dhammatalks.org importer
    wrote) to the SuttaCentral uid that now carries that text. The rules,
    applied in order, are:
    1. **Raw match:** a `raw_translations` row with `translator = 'than'`
       and `raw_sutta_id = <old ref>` whose `sutta_id` has `than` segments →
       that `sutta_id`. (This covers the 161 renumberings, e.g.
       `an10.165 → an10.176`, `an3.100 → an3.99`.)
    2. **Range containment:** the old ref lies inside the range of a
       segmented `than` `sutta_id` in the same group, or is the start of a
       raw range (`an1.140` → raw `an1.140-141` → `an1.140-149`;
       `an1.45` → `an1.41-50`; `an2.123` → `an2.118-129`;
       `sn22.127` → `sn22.126`).
    3. **Single-poem chapter:** `thagN` / `thigN` → `thagN.1` / `thigN.1`.
    4. **Split page:** a page that suttas-core split into several suttas
       (`sn43` → `sn43.1`, …) maps to its first sutta.
    5. **Override table:** a small hand-maintained table checked into the
       repository, for anything the rules above get wrong or cannot resolve.
       It is curated data: an entry that names a uid that does not exist must
       be reported, not silently ignored.
    6. Otherwise the old uid is **unmapped**.
21. Every dhammatalks.org record whose old uid is mapped must be **deleted**
    (its text now lives in the segmented record). The delete must go through
    Diesel so the FTS triggers and the `sutta_comments` cascade run.
22. Unmapped dhammatalks.org records must be **kept** as HTML records. With
    the 2026-10-31 data these are `kp1`–`kp9` (Khuddakapāṭha, not carried by
    suttas-core). The step must log every unmapped uid.
22a. With `--limit`, only the dhammatalks.org records whose new text was
    actually imported may be deleted; otherwise a limited run would delete
    texts it never replaced.
23. **Order of operations matters**, because old and new numbering collide
    (old `an3.100/en/thanissaro` is SC `an3.99`, while SC
    `an3.100/en/thanissaro` is the old `an3.101`). The importer must delete
    all mapped dhammatalks.org records **before** inserting any suttas-core
    Ṭhānissaro record, so no insert hits the `UNIQUE (uid)` constraint and no
    old record survives under a reused uid.
24. The step must log a summary: suttas-core `than` suttas imported, old
    records deleted, old records kept (unmapped), and a count per mapping
    rule.
25. The 23 suttas-core `than` raws with `sutta_id = NULL` need no records of
    their own: each is a piece of a merged raw that is segmented under a
    SuttaCentral range id (`an1.41-50`, `an2.1-10`, `an2.11-20`,
    `an2.21-31`, `an2.118-129`, `an2.32-41`, `sn22.126`). They are handled
    by the map (FR 20.2) only.

### 4.6 The uid map as shipped data

26. The old-to-new map of FR 20 must be stored in `appdata.sqlite3` in a new
    table, created by a **new Diesel migration** in
    `backend/migrations/appdata/` (e.g. `sutta_uid_renames` with columns
    `old_uid`, `new_uid`, `reason`). The bootstrap populates it.
27. **EPUB / HTML book imports** (`dhammatalk_org_convert_link_href_in_html`
    in `backend/src/helpers.rs`, used by `epub_import.rs`, the bootstrap's
    library imports and the user's own imports) convert dhammatalks.org links
    to `ssp://suttas/<old ref>/en/thanissaro`. That conversion must look the
    result up in the rename table and use the new uid. The library-imports
    bootstrap runs after the suttas-core step, so the shipped books get new
    uids.
28. User data is **not** remapped: bookmarks, saved reader sessions and
    `ssp://` links inside books already in a user's library keep their old
    uids after an upgrade. The app is new enough that this is accepted.
29. Every reader of the rename table must tolerate the table being absent (a
    migration failure is non-fatal by design; see
    `docs/database-migrations.md`): an absent table means "no renames".

### 4.7 Footnotes

The footnote display covers **every page with footnotes** that the reader
shows: the suttas-core records, the regular SuttaCentral comments (Sujato's,
Brahmali's, …, which today render as a `✱` mark that expands in place), and
the HTML pages and library books that already have footnotes — the
Ṭhānissaro Khuddakapāṭha pages left as HTML (`kp3`–`kp9`, `span.fn`),
Nyanadipa's Sutta Nipāta (16 pages) and library book chapters (58), both
`sup.footnote-reference`. They all load `assets/templates/page.html` and the
same footnote code. **Variant readings (`⧫`) and glosses keep their current
treatment.**

**The usability requirements, in order of importance** (FR 36):

1. A note appears in the bar at the bottom of the reader while its number
   is scrolled into view, and leaves when it scrolls out.
2. The note text in the bar can be **selected and copied**.
3. Sutta cross-links in a note **work** (open the sutta in the app).
4. The bar respects the **"show footnotes bar"** setting
   (`show_bottom_footnotes`).

Keeping footnote numbers out of the find bar's matches and out of copied
sutta text is a further benefit (FR 32a), not the goal.

30. The importer must import `segment_footnotes` for every record it writes
    (FR 13 replacements and Ṭhānissaro records; never a skipped record) into
    `sutta_comments`: one row per sutta
    record, `content_json` keyed by `segment_id`, `source_uid` and
    `language` set as for the SuttaCentral comments.
31. A segment can have several notes (up to 25 in the 2026-10-31 data), each
    with a `number`, an optional `word_index` (the word the note follows;
    `NULL` = the whole segment) and an `editor` flag. All of these must be
    kept. The value stored for a segment must be an HTML string with one
    element per note, in `number` order, carrying them as attributes, e.g.:

    ```html
    <span class='footnote' data-number='2' data-word-index='6'>According to Nd I, …</span>
    ```

    The exact form is decided during implementation, documented in the new
    doc (FR 44), and used consistently by the renderer and by
    `/sutta_bilara.json`.
32. **Rendering.** When the sutta renderer (the Bilara path in
    `backend/src/helpers.rs`, which today turns a comment into a `✱` mark
    with a hidden span) meets a comment value in this footnote form, it must
    instead:
    - insert a footnote reference after word `word_index` of the segment,
      counting words **exactly as suttas-core does**: the stored segment
      string split on `\s+`, empty tokens dropped, punctuation-only tokens
      counted (all 21,000-odd indexes in the 2026-10-31 data are in range
      under this rule). Insert before the word's trailing punctuation, so
      `disappeared:` renders as `disappeared²:` as on dhammatalks.org, and
      never inside an HTML tag. A note with no `word_index` goes at the end of
      the segment;
    - write the number as the link's **text**
      (`<span class='footnote-reference'><a href='#fn-…' id='fr-…'>2</a></span>`),
      the markup `is_footnote_link()` in `src-ts/footnote_modal.ts` already
      recognises. HTML served outside the reader (`/sutta_html` and the other
      API routes, read by agents and browsers without our CSS and JS) then
      still shows the numbers; FR 32a moves them out of the text in the
      reader only;
    - emit the footnote definitions at the end of the sutta with matching
      ids, in the form `get_footnote_content()` already reads.
32a. **Markers drawn by CSS, in the reader.** When a page loads, and again
    after a content swap, the reader's footnote code moves each footnote
    link's number from its text into a `data-n` attribute (brackets stripped:
    `[1]` → `1`) and empties the text; CSS draws it
    (`::after { content: attr(data-n) }`, superscript). This applies to every
    marker the bar observes (`.fn a[href^="#"]`,
    `.footnote-reference a[href^="#"]`), so it covers the new records, the
    HTML pages and the library books alike, with no change to stored data.
    The marker is shown as a **bracketed number in superscript**, `[1]`,
    `[2]`, … (`content: "[" attr(data-n) "]"`), on every page with
    footnotes, for visibility; the brackets are drawn, not stored, so the
    number in `data-n` stays plain.
    The find bar (`dom-find-and-replace`) and the snippet-aware find jump
    match across the page's text nodes, so a number left as text splits
    phrases: today `disappeared:` is not found in Ṭhānissaro's
    `disappeared²:`. With the numbers as attributes they are also left out of
    copied sutta text. The conversion must run before the bar starts
    observing, and the bar and pop-up must read the number from `data-n`
    (`extract_footnote_number()` reads the link text today).
33. Footnote ids must be unique on the page, including in the multi-column
    layouts where several texts are shown at once (include the record's uid
    or column index in the id).
34. Editor notes (`editor = 1`; their text already starts with
    `[Editor's note]`) must be shown, and must be visually distinguishable
    from the translator's notes (a CSS class is enough).
35. **SuttaCentral comments** (`sutta_comments` rows imported from
    SuttaCentral, one plain comment string per segment, with no number and
    no word position) must be rendered as footnotes too, instead of the `✱`
    mark: a footnote reference at the **end of the segment**, numbered in
    reading order within the sutta (1, 2, 3 …, shown as `[1]`, `[2]`, …
    like every other marker, FR 32a), and a definition at the end of the
    sutta, in the same markup as FR 32. Their stored data does not
    change; the renderer tells the two forms apart (FR 31's element vs. a
    plain string).
35a. Variant readings and glosses must keep rendering exactly as they do
    today, including their show/hide settings. The code that only served the
    `✱` comment mark must be removed, not left dead: `toggle_comment()` and
    its `.comment-wrap .mark` binding in `ssp_rebind_content_handlers()`
    (`assets/js/suttas.js`), and the `.comment-wrap` / `.comment` rules in
    `assets/sass/_suttacentral.sass` (keep the `.variant-wrap` / `.variant`
    half of the shared rules).
35b. `/sutta_bilara.json` keeps returning `comments` as stored; only the
    HTML rendering changes.
36. **Reader behaviour** (`src-ts/footnote_bottom_bar.ts`,
    `src-ts/footnote_modal.ts`), for every page with footnotes, as in the
    reader today for Ṭhānissaro's HTML pages (see
    `~/Screenshots/2026-11-02_13-13.png`):
    - **Shown while in view.** The bar shows each note while its marker is in
      view (the existing `IntersectionObserver`, threshold 0.5), in number
      order, and the pop-up opens when the marker is clicked. A marker
      without a text node must still intersect: its `::after` gives it a box.
    - **Selectable and copyable.** Today `update_display()` rebuilds the
      whole bar (`innerHTML = …`) whenever any note enters or leaves view,
      which destroys a selection the user is making in it, and showing the bar
      reflows the page, which can itself change which markers are in view.
      The bar must instead add and remove only the items that changed, so a
      selection inside a note survives every update that does not remove that
      note. The bar's text must not be made unselectable (no `user-select:
      none`, no `mousedown` `preventDefault`).
    - **Working links.** Links in a note keep working in the bar and the
      pop-up: `attach_link_handlers_to_element()` opens `ssp://suttas/…`,
      `https://suttacentral.net/…` (SuttaCentral's own comment links) and
      thebuddhaswords.net links in the app. With incremental updates, handlers
      are attached to each **new** item once, not re-attached to the whole bar.
    - **The setting.** With "show footnotes bar" off
      (`SSP.show_bottom_footnotes === false`) the bar stays hidden; markers
      still open the pop-up and the notes list at the end of the text is
      still there. Turning the setting on or off takes effect without
      reloading the page, as it does today (`footnote_bottom_bar_refresh()`).
    - **Multi-column.** When more than one column has notes, a bar item shows
      its column's label with the number, since each column numbers its own
      notes.
    - **Content swaps.** All of the above works after a content swap in the
      multi-column view (`window.ssp_rebind_content_handlers()` /
      `ssp-content-swapped`, see
      `docs/sutta-display-settings-and-multi-column-view.md`): FR 32a's
      conversion runs on the new content before the bar re-observes it.
37. **Links in footnotes.** suttas-core's note texts have no links (e.g.
    "see SN 36:11, AN 9:31, and AN 10:72"); the HTML pages had them. The
    **importer** turns sutta references in a note into `ssp://suttas/…`
    links when it builds the note, so the stored note carries its links, the
    renderer needs no database lookup, and `/sutta_bilara.json` serves linked
    notes. References in Ṭhānissaro's notes use his own numbering, so they are
    resolved through the in-memory old-to-new map. Matching is
    **case-sensitive**: Ṭhānissaro writes `SN 36:11` for the Saṁyutta and
    `Sn 4:11` for the Sutta Nipāta, so the case-insensitive
    `RE_ALL_BOOK_SUTTA_REF` cannot be used as it is. A reference that cannot
    be resolved stays plain text.
38. Footnote text must not be added to `content_plain`, so it does not change
    search results.

### 4.8 Survey command (porting back to suttas-core)

39. A new `bootstrap_cli` subcommand, `survey-suttas-core`, must compare the
    dhammatalks.org and SuttaCentral HTML sources with suttas-core and write a
    report. It is a separate command, not part of `bootstrap`, and must not
    write to any database.
40. Inputs: the dhammatalks.org mirror, the suttas-core database (FR 1), and
    an `appdata.sqlite3` built **without** the suttas-core step (or the
    importers run in memory), so the report sees the original HTML records.
41. The report must list, for Ṭhānissaro:
    - every old uid, its new uid, and which mapping rule (FR 20) produced it;
    - unmapped old uids and suttas-core suttas with no dhammatalks.org page;
    - per mapped sutta: title differences, and whether the plain text differs
      after whitespace and punctuation normalisation (with a short diff
      excerpt or a similarity figure);
    - footnote count differences.
42. For Bodhi and PTS: Simsapa records with no suttas-core counterpart, and
    suttas-core suttas that were skipped because Simsapa has no record of
    them (counts, with the uid lists in the file).
42a. For suttas-core labels whose Simsapa records are already segmented
    (Sujato, Brahmali, …): suttas-core suttas that Simsapa has no record of.
    These are not import candidates. A gap here means the `appdata.sqlite3`
    being surveyed lost records, or is keyed differently (Brahmali's Vinaya
    rules).
43. The report must be written to a file (a path given on the command line,
    defaulting to the scratch/output folder of the bootstrap), as Markdown
    or JSON — one machine-readable form is enough. It must **report, never
    repair**: it does not change suttas-core or the override table.

### 4.9 Documentation

44. A new doc, `docs/suttas-core-import.md`, must describe: the import rules
    per translator, the insertion point and why, the uid map and its rules,
    the rename table and its reader (book link conversion; footnote links
    are made by the bootstrap from the in-memory map), the footnote storage format and
    rendering, the survey command, and how to point `current` at a new
    suttas-core build.
45. `PROJECT_MAP.md` and the `CLAUDE.md` notable-docs list must be updated.
    `docs/simsapa-cli-and-localhost-api.md` must describe the footnote form
    in `/sutta_bilara.json`'s comments, and that Bodhi, PTS and Ṭhānissaro
    records now have `content_json`.

## 5. Non-Goals (Out of Scope)

- **Simsapa-side adjustments to Ṭhānissaro's texts** (custom numbering or
  text fixes beyond what suttas-core already applies): a later feature.
- **Surveying and cataloguing every diverging Ṭhānissaro translation**: a
  later feature. This PRD's survey command only produces the report.
- **Creating segmented versions in suttas-core** of Bodhi translations it
  lacks (`dn1`, `dn2`, `dn15`) or of the Khuddakapāṭha: a later contribution
  to suttas-core.
- **Patching suttas-core itself** from this repository.
- Importing suttas-core's other layers: `sutta_analysis`, `sutta_toc`,
  `sutta_tags`, the Vinaya tables.
- Replacing any record that already has `content_json`.
- Adding any non-Ṭhānissaro sutta that Simsapa would not otherwise have
  (FR 16's 5 Sujato suttas exist after a fresh bootstrap).
- Bumping the app or database version: the maintainer does this (a version
  bump is needed so installs re-download `appdata.sqlite3`).

## 6. Design Considerations

- Imported records render through the same Bilara template path as Sujato's
  translations, so the reading view, find bar, multi-column alignment and
  column bar work without per-source code.
- The footnote bar must look as it does now (screenshot
  `~/Screenshots/2026-11-02_13-13.png`): note number in brackets on the
  left, note text on the right, notes leave the bar when their marker
  scrolls out of view.
- Footnote markers are bracketed superscript numbers, `[1]`, `[2]`, …, on
  every page with footnotes, matching the `[1]` the footnote bar already shows
  beside each note.

## 7. Technical Considerations

### 7.1 suttas-core data facts (2026-10-31 build)

- Every `bodhi`, `than`, `pts` and `walshe` segment id is also a segment id
  of the Pāli `ms` text of the same sutta, and every such sutta has a
  `segment_structure` template. Alignment with the Pāli is therefore by
  key, like Sujato's.
- Translator counts with segments: `bodhi` 3,452 suttas, `than` 1,443,
  `pts` 3,246, `walshe` 34.
- Many segments were segmented by an LLM (`sutta_segments.source = 'llm'`)
  and are listed as unaudited in `build.log`. This is accepted; quality
  issues found are reported to suttas-core via the survey.
- Bodhi has 223 `abbreviated` raws, PTS 1,738: these are imported as they
  are.
- suttas-core trims trailing whitespace from some segments; in SuttaCentral's
  Bilara data that space separates words when segments are joined. The
  imported translations mostly keep it (Bodhi 49,961 and Ṭhānissaro 37,128
  segments end with a space), and the unspaced ones sampled are legitimate
  joins. Test that rendered text has no run-together words (e.g. `town.There`).
  Open questions about this, Brahmali's per-rule Pātimokkha ids and the
  missing `pli-tv-pvr17` are in `docs/suttas-core-questions-for-upstream.md`.
- `segment_footnotes.word_index` is `NULL` for 104 `than` notes and 314 `bodhi`
  notes (whole-segment notes and editor notes).

### 7.2 Ṭhānissaro uids change

Simsapa today keys Ṭhānissaro by the dhammatalks.org file name, which follows
his (often PTS) numbering: `an10.165/en/thanissaro` is the Cunda sutta that
SuttaCentral numbers AN 10.176. suttas-core has re-keyed every text to
SuttaCentral ids (`data/patches/than.json`: 161 `set_sutta_id`, 7 merges,
5 splits; see `suttas-core/docs/parsers/than.md`). Adopting those ids is
what makes Ṭhānissaro line up with the Pāli and the other translations, and
it is why the rename table (§4.6) and the delete-before-insert order (FR 23)
are needed.

Because the numberings collide, **a lookup that falls back to the rename table
only on a miss is wrong** for colliding uids (`an3.100/en/thanissaro` exists
under both numberings). That is why the rename is applied at the two points
where old-numbered uids enter the system (book link conversion and footnote
references), not as a general uid-resolution fallback. Existing user data
(bookmarks, sessions, library books) is deliberately not remapped (FR 28).

### 7.3 Other notes

- The SuttaCentral importer ignores SC's own `…/than` and `…/thanissaro`
  records (`uid_is_ignored()` in `suttacentral.rs`); that stays.
- FTS5 triggers on `suttas` keep `suttas_fts` in sync on insert, update and
  delete; the Tantivy index is rebuilt after the bootstrap from
  `content_plain`.
- `sutta_comments.sutta_id` has `ON DELETE CASCADE`, so deleted dhammatalks
  records take no comments with them (they have none today).
- Tests run against the dist `appdata.sqlite3` (see `CLAUDE.md`): a test that
  inserts rows must remove exactly those rows, using `dummy-…` uids.
- Unit tests are needed for: the uid-map rules (each rule, plus the
  `an3.99`/`an3.100` collision), `content_json` key order, word-index
  insertion (HTML inside a segment, trailing punctuation, `NULL` index), and
  the rename lookup with the table absent.
- Diesel's `appdata_schema.rs` must gain the new table; the qmllint stubs
  are unaffected unless a bridge function is added.

## 8. Success Metrics

With the 2026-10-31 suttas-core build:

- Bodhi: 1,165 of 1,168 Simsapa Bodhi records have `content_json`; the 3
  left (`dn1`, `dn2`, `dn15`) are HTML.
- PTS: 144 records replaced (horner 92, woodward 25, tw-caf_rhysdavids 12,
  tw_rhysdavids 11, hare 2, caf_rhysdavids 1, rhysdavids-brasington 1).
- No record that had `content_json` before the step has different
  `content_json` after it (check by comparing the two databases).
- SuttaCentral comments (e.g. `mn10/en/sujato`) show in the bottom footnote
  bar; variant readings still show as `⧫` marks.
- Ṭhānissaro: 1,443 segmented records; the only HTML records left are
  `kp1`–`kp9`; no SuttaCentral sutta has two Ṭhānissaro records.
- Comparing the databases before and after the step: the only uids added
  are Ṭhānissaro's, and the only uids removed are mapped dhammatalks.org
  Ṭhānissaro records.
- The navigation-tree builds report no more missing leaves than before.
- In the reader, tested by the user by hand on a Ṭhānissaro sutta
  (`snp4.11`), a Sujato sutta (`mn1`), a Nyanadipa page, `kp5/en/thanissaro`
  and a library book chapter with notes: notes appear in the bottom bar while
  their number is in view; a note's text can be selected and copied while
  scrolling a little; a sutta link in a note opens the sutta; with "show
  footnotes bar" off the bar stays hidden and clicking a number opens the
  pop-up; the find bar finds `disappeared:` in `snp4.11`.
- `cargo test` passes in `backend`, `bootstrap_cli`, `api`.

## 9. Open Questions

Resolved:

- *Footnote reference detection* (FR 37): a new case-sensitive pattern, applied
  by the importer; `RE_ALL_BOOK_SUTTA_REF` is case-insensitive and cannot
  tell `SN` from `Sn`.
- *Survey baseline* (FR 40): a separate `appdata.sqlite3` built without the
  suttas-core step; the command refuses a baseline that has rename rows.
- *Override table location* (FR 20.5): `assets/thanissaro-uid-overrides.toml`.
- *SuttaCentral comment numbering* (FR 35): numbered 1, 2, 3 in reading
  order, shown as `[1]`, `[2]`, … like every footnote marker (FR 32a).
- *Default English beside a Pāli text* (FR 16a): Sujato stays the default.

No open questions remain.
