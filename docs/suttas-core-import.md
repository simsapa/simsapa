# Segmented translations from suttas-core

The bootstrap step that replaces HTML-only translations with the segmented
(Bilara-style) texts of [suttas-core](https://git.unxr.net/adicco/suttas-core/),
the footnote form those texts bring, and the reader code that shows footnotes.

An HTML-only record (`suttas.content_html`, no `content_json`) cannot be lined
up with the Pāli in the multi-column view and cannot be served by `/sutta.json`.
A suttas-core text is keyed by the SuttaCentral segment ids of the Pāli root
(`ms`), so it aligns like Sujato's translation does.

## 1. Input and position

- Input: `<bootstrap assets>/suttas-core-db/current/suttas.sqlite`. `current` is
  a symlink to a dated folder (`2026-10-31`); the step logs the resolved path,
  and every record it writes gets `source_info = "suttas-core <dated folder>"`.
  The database is opened read-only. It is **required**: a missing file stops the
  bootstrap, checked up front by the preflight
  (`bootstrap_cli/src/bootstrap/preflight.rs`) before anything is written,
  because without it the release would silently lose its segmented English
  translations.
- To use a new suttas-core build: put its database in a new dated folder, repoint
  `current`, re-run the bootstrap.
- Position: in `bootstrap()` **after** the SuttaCentral and dhammatalks.org
  imports and **before** Dhammapada / Nyanadipa / Munindo, the library imports
  and the navigation-tree builds. The tree resolves each leaf's `sutta_uid`
  against rows that exist, so it must see the final uids; the library imports
  convert dhammatalks.org links through the rename table (§4), so the table must
  be filled by then.
- `--limit` limits the texts processed per label, and a limited run deletes only
  the dhammatalks.org records whose new text it actually imported.
- Code: `SuttasCoreImporter` in `bootstrap_cli/src/bootstrap/suttas_core.rs`.

## 2. Rules per translator

**The decision is made from the data, never from translator names.**

For every suttas-core label except `than`, and every segmented
`(sutta_id, language)`, the importer looks for Simsapa records
`<sutta_id>/<language>/<source_uid>`:

| Simsapa record | Action |
|---|---|
| HTML-only (`content_json` empty) | updated in place; `id`, `uid`, `title`, `sutta_ref`, range fields and `source_uid` are kept |
| already has `content_json` | skipped, untouched, no footnotes written |
| no record | skipped; Simsapa never gains a sutta it would not otherwise carry |

`source_uid` comes from `LABEL_SOURCE_UIDS`, which holds only the labels that
differ (`pts` → `horner`, `woodward`, `hare`, `tw_rhysdavids`,
`tw-caf_rhysdavids`, `caf_rhysdavids`, `rhysdavids-brasington`). It is used only
to *find* the record. A missing entry falls back to the identity and can only
miss a replacement, never cause an unwanted import. In practice this replaces
Bodhi's and the PTS translators' HTML records; a Bodhi HTML record without a
suttas-core counterpart (`dn1`, `dn2`, `dn15`) stays HTML.

A record is built as:

- `content_json`: **one key per key of the template, in template order**, the
  value from `sutta_segments` or `""`. Never sort the keys, and never pass the
  map through `serde_json::Value` (`IndexMap` + `serde_json::to_string`). The
  renderer walks the translation's keys and wraps each in its template entry, so
  a missing key would drop that entry's markup (`<section>`, `<h2>`, a closing
  `</p>`); 14 Ṭhānissaro texts lack segments the Pāli has.
- `content_json_tmpl`: the template of Simsapa's own `<sutta_id>/pli/ms`; when
  that is absent, built from suttas-core's `segment_structure` and logged.
- `content_plain`: through the helper the SuttaCentral importer uses
  (`bilara_html_and_plain_text()`), **without footnotes**, so footnote text
  never reaches search.
- `content_html = NULL`; `consistent_niggahita()` applied to every segment and
  note; `copyright` / `license` from `sutta_copyrights`.

Empty segments are kept. Trailing whitespace in segments separates words when
segments are joined; do not "clean" it.

### The default English beside a Pāli text

`default_english_for_pali()` (`backend/src/app_data.rs`) picks "the first
segmented English" in uid order, so `…/en/bodhi` and `…/en/horner` would sort
before `…/en/sujato`. It therefore prefers a segmented English whose
`source_info` does not start with `suttas-core`, then a suttas-core one, then any
other. This is why `source_info` is written.

## 3. Ajahn Ṭhānissaro

All 1,441 segmented `than` texts are imported, as `<sutta_id>/en/thanissaro`
with `source_uid = "thanissaro"`, under **SuttaCentral numbering**. The
dhammatalks.org importer keys his pages by file name, which follows his own
numbering (`an10.165/en/thanissaro` is the sutta SuttaCentral numbers AN 10.176),
and the two numberings collide (old `an3.100` is SC `an3.99`; SC `an3.100` is
old `an3.101`).

### The uid map

`thanissaro_uid_map.rs` maps each dhammatalks.org-derived reference to the
suttas-core `sutta_id` carrying its text. Rules, in order:

1. **override** — `assets/thanissaro-uid-overrides.toml` (curated data, embedded
   with `include_str!`, empty today); wins over everything. An entry whose `new`
   is not a segmented `than` text is reported as an error line and not applied.
2. **raw match** — a `than` raw with `raw_sutta_id` = the old reference. If the
   raw names a `sutta_id` with no segments, the reference is **unmapped**; no
   later rule may guess (`thag3.8`).
3. **range containment** — a raw range starting at the reference (`an1.140` →
   `an1.140-149`), a segmented range containing it (`an1.45` → `an1.41-50`), or a
   raw range containing it (`sn22.127` → `sn22.126`), same group only.
4. **single-poem chapter** — `thagN` / `thigN` → `thagN.1` / `thigN.1`.
5. **split page** — a page suttas-core split (`sn43` → `sn43.1`).

The map is computed **once, before anything changes the records it reads**, and
handed to both steps. With the 2026-10-31 data: raw_match 1,319,
range_containment 30, single_poem_chapter 9, split_page 1.

### Order of operations

One transaction: (1) delete every mapped dhammatalks.org record **through
Diesel** (FTS triggers and the `sutta_comments` cascade run); (2) insert the
segmented records, skipping a uid that already has `content_json`; (3) write
`sutta_uid_renames`; (4) rewrite old-numbered `ssp://suttas/…/en/thanissaro`
links inside the **kept** records. Deleting before inserting is what avoids the
`UNIQUE (uid)` collision of the two numberings.

Unmapped records are **kept as HTML** and each is logged: `kp1`–`kp9` (not in
suttas-core), `sn44` (the saṁyutta introduction page), `thag3.8` (empty text).
A text whose segments are all empty (`thag3.8`, `thag3.9`) is never imported.

Titles come from `raw_translations.title`; 15 texts (the split `sn43.*`) have
none and fall back to the mapped record's title, then `title_pali`, and are
logged.

### Introductions

suttas-core drops the introduction of each of his pages (its `than/page.ts`
`bodyStart()`). `thanissaro_intro.rs` cuts it out of the dhammatalks.org record
by the same rule and the import stores it in `suttas.description` (141 texts),
prepending its text to `content_plain`. The renderer shows it after the title
(single text) or above the grid, labelled (multi-column).

## 4. The rename table

`sutta_uid_renames(old_uid PRIMARY KEY, new_uid, reason)` — created by the
appdata baseline migration, filled by the Ṭhānissaro step; `reason` is the rule name.

Readers: `load_sutta_uid_renames()` and `rename_ssp_sutta_href()`
(`backend/src/sutta_uid_renames.rs`), used by
`dhammatalk_org_convert_link_href_in_html()` so EPUB / HTML book imports (the
shipped library and the user's own) write the new uid into
`ssp://suttas/<ref>/en/thanissaro` links. **An absent table means "no
renames"** — a migration failure is non-fatal by design.

The rename is applied at the two points where old-numbered uids *enter* (book
link conversion; footnote references, §5). It is **not** a general uid fallback:
colliding uids exist under both numberings, so a lookup that fell back only on a
miss would be wrong. The dhammatalks.org importer passes an empty map on
purpose (it writes old numbering). User data — bookmarks, saved sessions, links
in books already in a library — is not remapped.

## 5. Footnotes

### Stored form

`sutta_comments.content_json` maps `segment_id` → either a plain SuttaCentral
comment string, or one or more elements in number order:

```html
<span class='ssp-fn' data-n='2' data-w='6'>note HTML</span>
<span class='ssp-fn ssp-fn-editor' data-n='7'>[Editor's note] …</span>
```

`data-w` is the zero-based word index the note follows (absent = whole segment),
`ssp-fn-editor` marks an editor's note. `backend/src/footnotes.rs` encodes and
parses it, so bootstrap and renderer cannot drift; a value with no `ssp-fn` is
plain. `/sutta_bilara.json` returns `comments` as stored.

Sutta references in a note are turned into `ssp://suttas/…` links **by the
importer** (`footnote_sutta_links.rs`), so the renderer needs no database. The
pattern is **case-sensitive** (`SN 36:11` is the Saṁyutta, `Sn 4:11` is the
Sutta Nipāta) and never edits inside a tag or an existing `<a>`. Ṭhānissaro's
references are in his numbering and resolve through the in-memory old→new map,
then to a kept record, then to the Pāli. Unresolvable references stay text.

### Rendering

`bilara_text_to_segments()` (`backend/src/helpers.rs`) places notes:

- word index counted **exactly as suttas-core does**: the segment string split on
  `\s+`, empty tokens dropped, punctuation-only tokens counted. The marker goes
  after word `w` and **before its trailing punctuation** (`disappeared²:`), never
  inside a tag; an index past the end goes at the end and is logged;
- a plain SuttaCentral comment becomes a note at the end of its segment, numbered
  1, 2, 3 … in reading order;
- the marker is `<span class='footnote-reference'><a href='#fn-<prefix>-<n>'
  id='fr-<prefix>-<n>'>n</a></span>` (number as text, so HTML served outside the
  reader still shows it); the definitions follow the text in
  `<section class='ssp-footnotes'><ol>`. `<prefix>` is the record uid sanitised
  (`footnote_id_prefix()`), so ids are unique across columns; multi-column pages
  get one list per annotated column, headed by the column label.

Variant readings and glosses are untouched. The old `✱` comment mark and its
`toggle_comment()` are gone.

### In the reader (`src-ts/`)

- `normalise_footnote_markers()` (`footnote_modal.ts`) moves each marker's number
  from its text into `data-n` (brackets stripped) and empties the text; CSS draws
  `[n]` in superscript. It runs on load before the bar observes, and on swapped
  content before `footnote_bottom_bar.refresh()`. This covers all three markups
  (`.fn` on the Ṭhānissaro HTML pages, `sup.footnote-reference` on Nyanadipa and
  library books, the new `span.footnote-reference`), so the find bar matches
  across a marker and copied text has no numbers. `extract_footnote_number()`
  reads `data-n` first.
- `footnote_bottom_bar.ts` keeps **one DOM item per visible note**, adding and
  removing only what changed: rebuilding the bar with `innerHTML` destroys the
  user's text selection. Link handlers are attached once per new item. Nothing
  sets `user-select: none` or prevents `mousedown`. With
  `SSP.show_bottom_footnotes === false` the bar stays hidden and markers still
  open the pop-up; the setting applies live. Multi-column pages with notes in
  more than one column label each item with its column.
- While the bar has notes to show, a round eye button (`#footnoteBarToggle`,
  created by the bar's `init()`) sits above its top edge. It hides the bar
  without touching the setting, and stays, lowered to the bottom edge (above
  the column bar), to show it again. The hidden state lasts for the page,
  across content swaps; the bar keeps tracking the notes in view meanwhile. The
  button's offset comes from `--footnote-bar-height` on `<body>`, the bar's
  `offsetHeight` (0 when hidden), refreshed on every visibility change and by a
  `ResizeObserver`. Its click stops propagation so the page's word lookup does
  not see it.

## 6. The survey command

```sh
simsapa_bootstrap_cli survey-suttas-core --appdata <baseline appdata.sqlite3> \
    [--suttas-core <path>] [--output <file.json>]
```

Compares the dhammatalks.org and SuttaCentral records with suttas-core and
writes a JSON report (default `suttas-core-survey.json` in the bootstrap dist
folder), for porting fixes back to suttas-core. The baseline must be an
`appdata.sqlite3` built **without** this step; the command refuses one with
rename rows. It opens both databases read-only and **reports, never repairs**. It
lists, for Ṭhānissaro, every old → new uid with its rule, the unmapped ones, the
suttas-core texts with no dhammatalks.org page, and per text a title pair, a
normalised-text similarity with the first difference, and footnote counts; for
the other labels, the records skipped for each reason, as counts with uid lists.

Open questions for the upstream author are in
[suttas-core-questions-for-upstream.md](./suttas-core-questions-for-upstream.md).

## 7. Traps

- Never sort `content_json` keys.
- Delete mapped dhammatalks.org records **before** inserting; colliding numbers.
- Use the rename table at entry points only, never as a general fallback.
- Footnote text must not enter `content_plain`.
- A change to how `content_plain` is produced ships with a re-bootstrap and an
  index rebuild.
- An appdata DB version bump is needed so installs re-download the database.
