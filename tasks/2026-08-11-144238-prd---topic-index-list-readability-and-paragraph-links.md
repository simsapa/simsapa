# PRD — Topic Index list readability and paragraph links

Date: 2026-08-11

## 1. Introduction / Overview

The Topic Index window (`assets/qml/TopicIndexWindow.qml`) shows the CIPS
(Comprehensive Index of Pāli Suttas) index: bold **headwords**, indented
**sub-topic labels**, and under each label one or more blue **sutta links**.

Two problems:

1. **Grouping is ambiguous.** The sub-topic labels and the sutta links are
   spaced evenly, so a reader cannot tell at a glance whether a blue link
   belongs to the label above it or the label below it.
2. **The sutta links show raw segment ids.** A link reads `DN 33:1.11.0
   Saṅgītisutta`. The `:1.11.0` part is a SuttaCentral/Bilara *segment id* — it
   is noise to the reader, and it is not even used: clicking the link opens the
   sutta at the top, ignoring the location entirely.

This feature (a) restores the visual rhythm with a colon after each sub-topic
label and extra space above each label, and (b) hides the segment id from the
label while *using* it to scroll the opened sutta to that exact paragraph.

It also adds (c) a user-controlled **Show references** option in the sutta
cogwheel menu's *Layout* section, rendering the small segment numbers
(`1.1.1`, `1.1.2`, …) beside each paragraph the way SuttaCentral does. Today
those markers appear only as an undocumented side effect of navigating to an
anchor, reachable no other way; this makes them a deliberate, persisted
setting — while **keeping** the anchor-forces-them-on behaviour, which is
wanted: a reader arriving from the Topic Index needs to see which reference the
index entry took them to.

Two supporting pieces of work happen at **build time**, in the CLI's
`parse-cips-index` command, so that runtime does no extra work: the
disambiguation suffixes (§4.2) are computed and baked into the index JSON, and
every paragraph location is validated with the results printed as a warning
summary (§4.5).

## 2. Goals

1. A reader can tell which sub-topic a sutta link belongs to without counting
   pixels.
2. Sutta links read as plain references — `DN 33 Saṅgītisutta`, not
   `DN 33:1.11.0 Saṅgītisutta`.
3. Clicking a sutta link that carries a paragraph location opens the sutta
   **and scrolls to that paragraph**, with a brief highlight so the reader sees
   where they landed.
4. When two links under the same sub-topic resolve to the same visible label,
   they are still individually distinguishable and individually clickable.
5. Nothing regresses for links without a location, for cross-references
   (`• see: …`), or for suttas whose text has no segment ids.
6. A reader can turn the SuttaCentral-style per-paragraph reference numbers on
   or off from the cogwheel menu, and the choice persists.
7. The Topic Index window does no extra per-frame or per-click work for the
   above: the disambiguation suffixes are pre-computed at build time.
8. A broken paragraph location in the CIPS source data is caught when the index
   is generated, not by a reader clicking a link.

## 3. User Stories

- **As a reader browsing a topic**, I see `sense bases:` followed by an indented
  `SN 35.24 Pahānasutta`, with a blank line before the next label, so the
  grouping is obvious.
- **As a reader**, I click `DN 33 Saṅgītisutta` under *striving* and the sutta
  opens scrolled to the paragraph the index actually cited, briefly highlighted,
  instead of dumping me at the top of a very long sutta.
- **As a reader**, when one sub-topic cites the same sutta at two different
  paragraphs, I see `DN 33 Saṅgītisutta (a)` and `DN 33 Saṅgītisutta (b)` and
  can reach both places.
- **As a reader on an older text without segment markup**, the link still opens
  the sutta normally — nothing appears broken.

## 4. Functional Requirements

### 4.1 Layout and vertical rhythm

1. Each sub-topic label must be rendered with a trailing colon: `sense bases:`.
   The colon is presentation only — it must **not** be added to the underlying
   index data, and must not be included in search matching or in the text passed
   to `highlight_query_terms()`.
2. Extra vertical space must be inserted **above** each sub-topic label, so each
   `label: + its links` reads as one block separated from the next. Target
   shape:

   ```
   abandoning (pajahati, pahāna)
     sense bases:
       SN 35.24 Pahānasutta

     striving:
       DN 33 Saṅgītisutta

     things that should be abandoned:
       DN 34 Dasuttarasutta
   ```

3. The space above the label must **not** be applied to the first entry directly
   under a headword (no double gap under the bold headword).
4. Entries that have **no** sub-topic label (`sub` is empty or `"—"`) keep their
   current appearance and spacing. No colon is rendered for them, and no extra
   gap is introduced. Bold headwords and cross-reference lines (`• see: …`) are
   already visually distinct and must not change.
5. The existing indentation of links relative to their label must be preserved
   or slightly increased so the parent/child relation is still readable, and the
   headword-highlight rectangle must continue to cover the whole entry (its
   height is driven by `headword_column.height`).

### 4.2 Reference label formatting

6. A sutta reference must be displayed **without** its segment id: the displayed
   text is derived from the part before the `:` only. `dn33:1.11.0` displays as
   `DN 33`; `sn35.24` displays as `SN 35.24` (unchanged).
7. The existing space insertion between the collection letters and the number
   (`format_sutta_ref()`) is retained.
8. The sutta title, when present, still follows the reference:
   `DN 33 Saṅgītisutta`.
9. **Disambiguation suffix.** Within a single sub-topic entry (one `refs`
   array), if two or more `sutta` references produce the **same displayed label**
   (same reference *and* same title after stripping the segment id), each of
   them must be suffixed with a lowercase letter in parentheses, in the order
   they appear in the data: `DN 33 Saṅgītisutta (a)`, `DN 33 Saṅgītisutta (b)`,
   … If more than 26 collide, continue `(aa)`, `(ab)`, … (a simple
   base-26 continuation is acceptable; the case is not expected to occur).
10. **The suffix is computed at build time, not at runtime.** It is calculated by
    the CLI `parse-cips-index` command and stored in the generated index JSON as
    a new optional per-ref field (see §4.5). The QML delegate must do nothing but
    append the value it is given, so no per-click or per-delegate collision scan
    happens in the UI.
11. The suffix must only be added when there is an actual collision. A label
    that occurs once must have no suffix field (or an empty one) and must never
    show `(a)`.
12. Collision detection is scoped to the sub-topic entry only, never across
    entries or headwords.
13. Cross-references (`type === "xref"`) are unaffected by requirements 6–12.

### 4.3 Opening a sutta at a paragraph

14. When a sutta reference carries a segment id (the part after `:` in
    `sutta_ref`), clicking the link must open the sutta **and scroll to the
    corresponding paragraph**.
15. The segment id must be passed through the existing `anchor` field of the
    result-data JSON, not through the current `segment_id` field, which nothing
    reads. Concretely, `TopicIndexWindow.open_sutta()` must set
    `anchor: <full segment id, e.g. "dn33:1.11.0">`. The obsolete `segment_id`
    key must be removed once the anchor path works, so there is one mechanism
    and not two.
16. The anchor must be honoured in **both** open modes:
    - "Open in new window" checked → `open_sutta_search_window_with_result()`
    - unchecked → `emit_show_sutta_from_reference_search()`
17. Requirement 14 must work when the target sutta is **already open** in the
    reading panel — clicking a second location of the same sutta must re-scroll
    rather than do nothing. (The existing snippet find-bar path has the same
    problem/solution shape; see §6.)
18. **Highlight.** After scrolling, the target segment must be given a brief
    background highlight that fades out after roughly 1–2 seconds. It must be
    theme-aware (readable in light and dark), and must not alter layout (no
    border/size change that would shift the text).
19. **Nearest-preceding-sibling fallback.** If no element with the segment id
    exists, the app must look for the nearest preceding paragraph **within the
    same parent section**, in this order, taking the first that exists:
    1. decrement the **last** numeric component: for `dn33:1.7.9.10` try
       `1.7.9.9`, `1.7.9.8`, … down to `1.7.9.0`;
    2. then the parent itself, exactly once: `dn33:1.7.9`.

    Then stop. It must **never** walk up past that (no `1.7.8`, no `1.7`) —
    a sibling of the parent may be an entirely different chapter of the sutta,
    which would silently take the reader somewhere misleading. A resolved
    fallback target is scrolled to and highlighted exactly like an exact hit.

    **Step 2 is a safety net that the current data never exercises, and that is
    expected.** Verified against the shipped database: neither `dn33:1.7.9` nor
    `dn20:4` exists as a segment key — Bilara emits headings as `x.y.z.0`, never
    as the bare parent. Both measured failure classes (§4.5) resolve at step 1,
    and so do both acceptance cases in §8.5. Keep step 2 (it is two lines and
    costs nothing), but do **not** spend time debugging why it never fires, and
    do not read §8.5 as evidence that it works.
20. **Give up cleanly.** If neither step of requirement 19 finds an element —
    including the case of a text with no segments at all — the sutta must open
    normally, scrolled to the **top**, and a notice must be shown **at the top
    of the content**, reading:

    > Referenced location `dn20:4.11` not found.

    No fallback sentence, because there was no fallback. There is **no error
    dialog** and nothing that interrupts reading; the notice is the whole of the
    user-facing signal, and it tells the reader why they are at the top of the
    sutta rather than at a passage.
21. The give-up notice is the **same component** as the fallback notice, with
    the second sentence omitted: same styling, same readable size, same
    no-auto-fade rule, same dismissible "×", same selectable text, same
    single-instance rule. Requirements 26–36 apply to it unchanged except for
    placement — it goes at the top of the content rather than before a resolved
    paragraph.
22. Both outcomes are also logged: a miss with `logger.warn()` naming the uid
    and the anchor that was not found; a *successful* fallback with
    `logger.info()` naming both the requested and the used segment id — so a
    support log distinguishes "exact", "approximate" and "missed".
23. The candidate walk must operate on ids **as they exist in the loaded page**,
    not on a list computed in Rust — the page is the only authority on which
    segments the currently displayed text actually has, and it covers the
    translation-without-that-segment case for free.
24. A location whose last component is **not numeric** must skip step 1 and go
    straight to step 2. It must never crash or loop.

**In-page notice.** One component covers both imperfect outcomes. A fallback
jump is an approximation and the reader must be told so at the place they land,
or they may read the wrong paragraph believing it is the cited one; a give-up
leaves them at the top of a sutta with no explanation unless they are told.

25. When the jump was resolved by the fallback of requirement 19, a notice must
    be inserted into the page at the resolved location, reading:

    > Referenced location `dn20:4.11` not found. This location `dn20:4.10` is
    > the closest fallback.

    Both locations must appear, the requested one first. Showing them is the
    point of the notice — and it is coherent here because a fallback jump always
    arrives with the reference numbers rendered (requirement 43, rule 2), so the
    reader can match `4.10` against the label printed beside the paragraph.
26. **The message must be real, selectable text** — ordinary elements and text
    nodes, not CSS `content:` — so the reader can select it, copy it, and paste
    it into an email reporting the bad location. Nothing about the notice may
    make its text unselectable (no `user-select: none` on the message; the "×"
    control may exclude itself).
27. Because the message is real text under `#ssp_content`, the find bar will
    walk it (`find.ts:73`, `:357`) and can highlight and count its words. **That
    is accepted**: the wording is navigational, not scriptural, so it is
    unlikely to collide with what a reader types into the find bar. Do not
    contort the markup to avoid it.
28. The locations must be written in **full** (`dn20:4.11`), not the short form
    the page prints in the margin. The notice is meant to be copied into a
    report, where the sutta must not be left implied — and the short form
    remains visible as a substring for matching against the label beside the
    paragraph. The log line of requirement 22 uses the same full form.
29. The notice must be at **readable body size** — not the small `.reference`
    type — and theme-aware in light and dark, following the same token/colour
    approach as the existing in-page notices.
30. The notice must **not auto-fade and must not time out**. It stays until
    dismissed, so it cannot vanish while being read.
31. The notice must carry a **dismiss control** ("×") which removes it from the
    page on click. Dismissal is not persisted anywhere — a later jump that
    misses shows a notice again.
32. There must be **at most one** notice in the page at a time: inserting one
    must first remove any existing notice, so repeated jumps do not stack them,
    and a later exact hit does not leave a stale notice from an earlier miss
    on screen.
33. A **fallback** notice must be inserted as a **sibling before the resolved
    segment's nearest block-level ancestor** (`p`, `li`, `h1`…), never inside
    `<span class="segment">` — that span is the multi-column grid container
    holding the per-column `colcell` spans, and injecting a block into it breaks
    the Columns and Lines layouts. A **give-up** notice goes at the top of the
    content instead, as the first child of `#ssp_content`, above the sutta
    title.
34. The notice must be inserted **before** the scroll is performed, so that the
    scroll accounts for the height it adds and the target paragraph still lands
    in view. For the give-up notice this also guarantees it is the first thing
    in view, since the page is at the top.
35. **No notice is shown when the jump succeeds exactly**, and none when the
    reference carries no location at all (requirement 37) — nothing went wrong
    in either case.
36. A content re-render (a Layout / Repeat Pāli / references change, which
    replaces the children of `#ssp_content`) removes the notice as a side
    effect. That is acceptable and must **not** be worked around by recreating
    it — the reader has by then seen it.
37. A reference **without** a segment id (e.g. `sn35.24`) must behave exactly as
    it does today: open the sutta at the top, no anchor query parameter, no
    highlight.
38. Verse references that go through `convert_verse_ref_to_uid()` (dhp/thag/thig)
    must keep working; the anchor, if present, is applied after the uid
    conversion.

### 4.4 "Show references" display option

The per-paragraph reference numbers already exist in the renderer
(`show_references` → `generate_reference_anchor()`, `helpers.rs:2191`), producing
`<span class="reference"><a class="sc" id="1.11.0" href="#1.11.0">1.11.0</a></span>`
— the SuttaCentral-style small grey labels in the reference screenshot. Today
that is their *only* trigger: they are switched on whenever the sutta route
receives an `anchor` parameter (`api.rs:731`,
`let show_references = anchor.is_some();`), and a reader who wants them
otherwise has no way to ask. The anchor behaviour is kept and made deliberate
(requirement 43, rule 2); what is added is a user setting underneath it.

39. A **Show references** control must be added to the **Layout** section of the
    in-page cogwheel settings panel (`assets/templates/display_settings.html`,
    the `ds-section` whose header is `Layout`), alongside Layout / Repeat Pāli /
    Width. A two-state segmented control (`Off` / `On`) consistent with the
    neighbouring `ds-segmented` controls is preferred over a bare checkbox.
40. The setting must be **persisted** as a new field on
    `SuttaDisplayDefaults` (`backend/src/app_settings.rs:469`), defaulting to
    **off**, so existing installs see no change until they opt in.
41. It must obey the panel's existing **scope** semantics: "Save as default"
    persists it via `POST /save_sutta_display_settings`; "This view only"
    applies it to the current view without persisting.
42. Toggling it must re-render the sutta content through the existing
    content-swap path (`/sutta_content_block` + `reinit_sutta_content()`), with
    the same post-swap re-init contract as the other layout settings — not a
    full page reload.
43. `show_references` must become a genuine display option resolved like the
    others, with this **precedence, highest first**:
    1. an explicit `show_references` request parameter (what the cogwheel's
       content-block re-fetch sends — this is how a user can turn references
       *off* on a page that was opened with an anchor);
    2. **an anchor navigation target on the request** — an `anchor` parameter
       forces references on for that render, regardless of the persisted
       default, so the reader can see which reference they landed on;
    3. the persisted `SuttaDisplayDefaults` value.

    The existing `let show_references = anchor.is_some();` (`api.rs:731`) is
    therefore *narrowed*, not deleted: it stays as rule 2 and gains rule 3 under
    it. Note the current signature makes this awkward —
    `resolve_sutta_display_options(&sutta, show_references: bool, &overrides)`
    takes a plain forced `bool` (`app_data.rs:388`), so it cannot express
    "unset, use the default". It must become an `Option<bool>` (or move into
    `SuttaDisplayOverrides` alongside `layout` / `columns` / `repeat_pali`,
    which is the more consistent shape). Three call sites are affected:
    `api.rs:741`, `api.rs:1667`, and `app_data.rs:404`.
44. **The scroll must not depend on the setting.** Scrolling to a paragraph
    (§4.3) targets the `<span class="segment" id="dn33:1.11.0">` wrapper, which
    `bilara_text_to_segments()` emits regardless of `show_references`; the
    reference anchor is separate markup *inside* that span. This must be
    verified with references **off** rather than assumed, because today's
    coupling makes the two indistinguishable in practice: with rule 2 above,
    every Topic Index arrival has them on. The testable path is to open from the
    Topic Index and then switch references **off** in the cogwheel — the
    content-block re-render must keep the page usable and a subsequent jump to
    another location in the same sutta must still scroll correctly.
45. Turning the option **off** while viewing an anchor-navigated page must win
    (precedence rule 1) and must not be silently re-forced on by a later layout
    or Repeat Pāli change — those re-fetches carry the client's current
    `show_references` value, so the user's choice must be what they carry.

    **Scope limit, accepted.** That choice lives in the page, not in the tab.
    Any *full* reload of the same tab rebuilds the URL from the QML wrapper's
    still-set `root.anchor` (`SuttaHtmlView_Desktop.qml:137-142`), so rule 2
    fires again and the references come back on. The cheap remedy is to clear
    `root.anchor` after a jump resolves; either do that, or accept the
    behaviour deliberately — but do not leave it undecided, because "I turned
    them off and they came back" is a defect report either way.
46. The cogwheel control must initialise from the **effective** value the page
    was rendered with (`window.SUTTA_DISPLAY.show_references`), which on an
    anchor-navigated page is `true` even when the stored default is `false`.
    Initialising the control must **not** write that value back to the persisted
    default — only a user interaction persists (requirement 41).
47. When references are **on**, the reference number of the segment navigated to
    may additionally be emphasised, but this is optional polish; the paragraph
    highlight of requirement 18 is the required feedback.

### 4.5 CLI `parse-cips-index`: pre-computation and validation

All of this happens in `cli/src/bootstrap/parse_cips_index.rs`, which already
has a `ValidationResult { warnings, errors }` type and prints warnings at the
end of `parse_cips_to_json()` (`:604`).

**Measured shape of the data** (from `assets/general-index.json` and the
shipped `appdata.sqlite3`, 2026-08-11 — these are the numbers the
implementation should reproduce). **All of them were independently
re-derived during the PRD review and reproduce exactly**, including the
collision grouping under the key `(reference-before-the-colon, title)`, which
is what fixes the collision key of requirement 9:

- 19,970 `sutta`-type refs in total; only **1,579 (7.9 %) carry a segment id**.
  The paragraph-jump feature therefore applies to a small, DN-heavy minority —
  everything else keeps today's open-at-top behaviour.
- **25** colliding label groups involving **55** refs, largest group 5
  (`feet` / `Buddhas'` → five `dn30` locations). **No group is an exact
  duplicate** — every colliding ref has a distinct segment id, so the suffixes
  are always meaningful and never mask a redundant row. The `(aa)` overflow
  case does not occur.
- Anchor validation dry run: **1,573 ok, 6 missing segment, 0 unresolved uid,
  0 no-segments**, touching only **32 distinct suttas** (all segment-carrying
  refs are in DN). The six failures are genuine off-by-one data errors —
  `dn33:1.7.9.1` (the text has `1.7.9.0`) and `dn20:4.11`–`4.15` (the text
  stops at `4.10`).

48. **Suffix pre-computation.** After the index is built and before it is
    serialized, the parser must compute the disambiguation suffixes described in
    §4.2 (requirements 9–13) and store each one on its ref as a new optional
    field, e.g. `"suffix": "a"`. The field must be omitted (via
    `skip_serializing_if = "Option::is_none"`, matching the sibling fields in
    `TopicIndexRef`) when there is no collision, so the generated JSON does not
    grow for the common case.
49. The matching optional field must be added to
    `TopicIndexRef` in `backend/src/topic_index.rs` so the runtime deserializes
    and forwards it to QML. Both copies of the struct (the CLI's and the
    backend's) must stay in sync.
50. **Anchor validation.** For every `sutta`-type ref that carries a segment id,
    the parser must check that the segment id actually exists in the referenced
    sutta's content, and emit a warning naming the headword, the sub-topic and
    the failing `sutta_ref` when it does not.

    **The `content_json` keys are full segment ids** — `"dn33:1.11.0"`, with the
    uid prefix included, not `"1.11.0"`. The comparison is therefore against the
    whole `sutta_ref` string, unsplit. Splitting on `:` and comparing the tail
    reports all 1,579 refs as failures.

    **What is validated is `content_json`; what the reader's page actually
    carries an `id` for is `content_json_tmpl`.** `bilara_text_to_segments()`
    emits `<span class="segment" id="…">` only for keys that have a template
    entry — a key present in `content_json` but absent from `content_json_tmpl`
    renders as bare content with **no id at all** (`helpers.rs:~2316`). So this
    validation is an approximation of what the page has. Measured: across all 32
    referenced suttas, **0** content keys lack a template, so the two agree
    today. The gap is covered at runtime by requirement 23 (the walk reads ids
    from the loaded page, which is the only authority) — do not try to close it
    in the CLI.
51. The sutta whose segments are checked must be resolved **the same way the
    runtime resolves it** — `{uid}/pli/ms` first, matching
    `AppData::get_full_sutta_uid()` (`backend/src/db/appdata.rs:258`) — or the
    validation will not describe what a reader actually experiences.
52. Validation must distinguish and report separately, since the causes and the
    fixes differ:
    - the sutta uid could not be resolved at all (bad reference in the CSV);
    - the sutta was resolved but has **no segmented content** (an empty
      `content_json`, i.e. a legacy text — expected for some collections, and
      not necessarily a data error);
    - the sutta has segmented content but **not** that segment id (a genuinely
      wrong location — the 6 measured failures are all of this kind).
53. The warnings must be printed as a **summary at the end of the run**, after
    the existing validation warnings, with counts per category, followed by the
    individual warning lines. Against today's data the summary would read
    `Anchor validation: 1579 checked, 1573 ok, 0 unresolved uid, 0 no segments,
    6 missing segment`. A run with no problems must say so in one line rather
    than printing nothing.
54. Anchor validation failures are **warnings, never errors** — they must not
    fail the command or prevent the JSON from being written. The index ships
    with whatever the source data says; the runtime fallback of requirements 19–21
    covers the rest. (The six known failures must therefore still produce a
    complete JSON.)
55. Validation needs the suttas' `content_json`, which the existing
    `title_lookup` closure does not provide — it is built in
    `parse_cips_index_command()` (`cli/src/main.rs:770-822`) as a
    pre-loaded `HashMap<uid, title>` over all pli/ms suttas. Extend that
    mechanism (a second closure, or one closure returning both) rather than
    opening a second database connection. **Load segment keys lazily, per
    referenced uid, and cache** — the measured working set is 32 suttas.
    (Eagerly loading all 7,285 blobs is *affordable* — measured 36 MB, 0.12 s
    to fetch and 0.12 s to parse — so this is a tidiness choice, not a
    necessity. §6.11 records the measurement; do not reject the lazy approach
    on the grounds that eager loading would be catastrophic, because it would
    not be.)
56. **Uid existence needs no new query — but not the title map.** The existing
    map is *not* a safe existence oracle: it inserts only rows whose `title` is
    `Some` (`cli/src/main.rs:~805`), so a pli/ms sutta with a NULL title would
    be misreported as "unresolved uid". Today there are **0** such rows (checked
    against the shipped database, so the measured figures stand), but the
    dependency is invisible and would break silently. Collect a separate
    `HashSet<String>` of uids in the **same** loop over the same query result —
    no extra query, no per-ref `SELECT` — and answer "unresolved uid" from that.
57. If the command is invoked **without** `--db-path` (already supported — the
    title lookup degrades to `|_| None` and titles come out empty), anchor
    validation must be **skipped with one clear message**, not crash and not
    report 1,579 false failures.
58. The suffix computation must be unit-tested in the CLI's existing `mod tests`
    (no collision → no field; two colliding → `a`/`b`; a five-way group →
    `a`–`e`; a group that differs only by title → no suffix).

### 4.6 The tooling reports; the author decides

**Standing rule for everything in §4.5 and for the CIPS pipeline generally: the
parser must never repair, rename, normalize away or silently work around a
defect in the source data.** It reports; the report goes to the index author,
who evaluates it. This applies to wrong paragraph locations, to headword
spellings (the `conditions (saṅkāra)` typo, §C of the corrections document), to
cross-reference targets that do not resolve, and to anything found later.

The reasons are that the author is the only one who can tell a typo from a
deliberate reading, that a silent in-parser correction makes the CSV and the
shipped index disagree so the next reviewer re-finds the same problem, and that
an auto-correction is invisible in the diff of the generated JSON — the very
artefact §6.1 asks to be verified by diffing.

Three consequences to keep straight:

59. The generated JSON must reproduce the source strings **verbatim** —
    headwords, sub-topics and `sutta_ref` values. Existing transforms that are
    *presentational or internal* stay as they are and are not what this rule is
    about: the lowercasing in `parse_sutta_ref()`, `latinize()` in the sort
    keys, `make_normalized_id()` for `headword_id`. None of them changes the
    stored text.
60. Warning lines must quote the offending value **exactly as it appears in the
    data**, including its typos, so the author can find the row. Never print a
    "did you mean" substitution in place of the value.
61. The disambiguation suffixes of §4.2 are the one thing this feature *adds* to
    a ref, and they are additive metadata, not a correction: they never alter
    `sutta_ref` or `title`, and a colliding pair remains two rows (de-duplication
    was explicitly rejected — see Non-Goals).

## 5. Non-Goals (Out of Scope)

- Adding segment ids to texts that do not have them, or generating synthetic
  anchors for legacy HTML suttas.
- *Correcting* wrong paragraph locations found by §4.5 validation — this
  feature only reports them.
- Any change to the reference markup itself (`generate_reference_anchor()`) or
  to how the reference numbers are styled beyond what already exists.
- Making the reference numbers clickable to copy a link, or any other new
  behaviour attached to them.
- Walking **past the parent section** when a location is missing (`dn33:1.7.9.4`
  → … → `dn33:1.7.9` is in scope per requirement 19; continuing to `1.7.8` or
  `1.7` is **not**). A sibling of the parent can be an unrelated chapter, and
  landing there silently is worse than landing at the top.
- Repairing the source data as part of this feature — and, per §4.6, repairing
  it *anywhere in the tooling*, at any later point. The six known-bad locations
  and the misspelled `conditions (saṅkāra)` headword are documented for the
  index author in
  [2026-08-11-144238-cips-paragraph-location-corrections.md](./2026-08-11-144238-cips-paragraph-location-corrections.md);
  the fallback is a safety net, not a substitute for that review. **No
  auto-correction, no auto-rename, no normalization table, no "did you mean"
  substitution** — not in the parser, not in the validator, not in the runtime.
- Any change to the search behaviour, the A–Z navigation, or the Info dialog.
- De-duplicating repeated sutta references (explicitly rejected in favour of the
  `(a)` / `(b)` suffixes).
- Showing the segment id anywhere in the UI, including as a tooltip.
- Anchor support for dictionary words or book chapters (book chapters already
  have their own anchor path and are not touched).

## 6. Technical Considerations

The paragraph-jump mechanism **mostly exists already** — the missing piece is
that the Topic Index does not use it.

**What exists:**

| Piece | Where |
|---|---|
| `anchor` carried in result data through tab creation | `SuttaSearchWindow.qml:367` (`new_tab_data`), `:1228` (existing-tab update), `SuttaStackLayout.qml:56` |
| `anchor` property on the webview wrappers | `SuttaHtmlView_Desktop.qml:23`, `SuttaHtmlView_Mobile.qml:44` |
| Anchor appended to the sutta URL as `?anchor=…#…` | `SuttaHtmlView_Desktop.qml:137-142`, `_Mobile.qml:247-251` |
| JS fallback scroll after load | `scroll_to_anchor()`, `SuttaHtmlView_Desktop.qml:196-226` |
| Server-side `anchor` query parameter on the sutta route | `bridges/src/api.rs:779`, `:1613` |
| Segment elements in the rendered HTML | `backend/src/helpers.rs:2311` — `<span class="segment" id="dn33:1.11.0">` |
| Reference-number markup (the SuttaCentral-style labels) | `generate_reference_anchor()`, `backend/src/helpers.rs:2191` |
| `show_references` threaded through the render path | `backend/src/sutta_display.rs:34`, `app_data.rs:322/587`, `api.rs:1651` (`/sutta_content_block` already takes it as a query param) |
| Display-settings panel markup, Layout section | `assets/templates/display_settings.html:26-50` |
| Display-settings client state + persistence | `src-ts/display_settings.ts`, `POST /save_sutta_display_settings` |
| **`show_references` already round-trips through the client** | `SUTTA_DISPLAY.show_references` is injected by `sutta_display_js()` (`app_data.rs:654`), read by `refetch_with_params()` (`content_reload.ts:223`), sent by `build_content_block_url()` (`:26`) and written back after a swap (`:130`) |
| Persisted display defaults | `SuttaDisplayDefaults`, `backend/src/app_settings.rs:469` |
| CIPS parser with a validation-warning summary | `cli/src/bootstrap/parse_cips_index.rs` — `validate_index()` `:507`, warnings printed in `parse_cips_to_json()` `:604` |

**What is missing / needs care:**

1. **`TopicIndexWindow.qml:203` writes `segment_id`, which no consumer reads.**
   This is the whole bug behind requirement 14. Rename to `anchor`.
2. **`show_references` is much closer to done than it looks — the missing piece
   is the UI control and the persisted default, not the plumbing.** The value is
   already resolved into `SuttaDisplayOptions`, injected into the page as
   `window.SUTTA_DISPLAY.show_references`, carried by every content-block
   re-fetch and written back after each swap. So the "forced on by anchor" state
   already survives a Layout or Repeat Pāli change today, and requirement 45
   falls out of the existing code rather than needing new logic.

   What is genuinely missing: the segmented control in the panel, the
   `SuttaDisplayDefaults` field, the save path, and rule 3 of the precedence in
   requirement 43. Two traps: the full-page sutta routes (`api.rs:779`, `:1613`)
   have **no** `show_references` parameter at all — only `/sutta_content_block`
   does (`:1650`) — and `resolve_sutta_display_options` takes a plain `bool`
   that cannot express "unset". The renderer already consumes
   `options.show_references` (`app_data.rs:587`), so nothing in `helpers.rs`
   changes.
3. **Colons in selectors.** `dn33:1.11.0` is a valid `id` but **not** a valid CSS
   selector fragment. `document.getElementById()` (tried first) is fine;
   the third branch of `scroll_to_anchor()` runs
   `document.querySelector('${root.anchor}')`, which will throw a
   `SyntaxError` for a colon-bearing anchor and abort the IIFE. Wrap that branch
   in `try/catch`, or use `CSS.escape()`, so the miss is a clean `false` return
   (requirement 20) rather than a JS exception.
4. **URL fragment.** A `#dn33:1.11.0` fragment is legal, and the code already
   `encodeURIComponent`s the query-parameter copy. Confirm the fragment half is
   not double-encoded such that native scrolling silently fails.
5. **Highlight implementation.** Prefer a CSS class applied by injected JS
   (e.g. `.ssp-anchor-highlight` with a CSS animation/transition that fades out),
   defined in the Sass sources under `assets/sass/` alongside the existing
   `.ssp-find-highlight` rules (`assets/sass/_find.scss:242-258`), which already
   solve the light/dark pair. Remember `make sass` after editing.
   Do **not** hand-write CSS into `assets/css/`.
6. **Already-open sutta — the same-uid rule does *not* transfer from the find
   bar.** `show_result_in_html_view()` handles "same uid already displayed, page
   will not reload" for the find bar (`SuttaSearchWindow.qml:1096-1108`), and it
   is tempting to copy that condition. **Do not.** The anchor is part of the
   URL: `onData_jsonChanged` → `load_sutta_uid()` builds
   `…/uid/?anchor=X#X` (`SuttaHtmlView_Desktop.qml:137-142`), so a *different*
   anchor on the same uid changes the URL, genuinely reloads, and the existing
   `scroll_timer` already fires. Requirement 17 is therefore satisfied by the
   existing machinery for that case.

   The only case that does nothing is **same uid *and* same anchor** — an
   identical `data_json` string, so `onData_jsonChanged` never fires at all.
   That is what §6.15 is about, and it is the only case that may call
   `scroll_to_anchor()` directly.

   Getting this wrong is not cosmetic. A direct call keyed on the uid alone runs
   **before** the pending reload, against the *previous* page's DOM — and once
   the notice of requirements 20–36 exists, it can insert a notice that the
   incoming page then discards, or scroll the outgoing page. Compare **uid and
   anchor together**.
7. **Mobile parity.** `SuttaHtmlView_Mobile.qml` has its own copy of the anchor
   logic (native `QtWebView`, `runJavaScript` availability differs). Both
   desktop and mobile paths must be handled; where mobile cannot do the same
   thing, requirements 19–20 apply.
8. **Which text gets opened.** `get_full_sutta_uid()` may resolve to a
   translation. Bilara segment ids are shared between root and translations, so
   the anchor is expected to resolve either way — verify with at least one
   translation, and treat a miss under requirements 19–20.
9. Layout changes are pure QML in the `Repeater` delegate at
   `TopicIndexWindow.qml:448-522`; use `Layout.topMargin` on the sub-topic
   `ColumnLayout` rather than changing the `ListView.spacing`, which would also
   space out the headwords.
10. **The suffix field crosses a rebuild boundary.** The index JSON is embedded
    at build time (`CIPS_GENERAL_INDEX_JSON`, `backend/src/app_settings.rs`) and
    parsed by `topic_index.rs` into a `OnceLock` cache. Adding the field means:
    regenerate the JSON with `parse-cips-index`, add the optional field to
    **both** `TopicIndexRef` structs, and rebuild. Because the field is
    optional, an older JSON against newer code still parses — which is what
    makes the two halves independently landable.
11. **Anchor validation needs sutta content, which the parser does not have
    today.** `parse_cips_index()` takes a `title_lookup` closure
    (`parse_cips_index.rs:404`, `:564`); the natural shape is a second closure
    (or a widened one) returning the segment-id set for a uid, so the parser
    stays testable without a database and requirement 57's skip path is a
    `None` rather than a special case. Cache per uid — measured working set is
    32 suttas out of 7,285 (requirement 55).

12. **Two different `id` attributes exist in the rendered page, and only one is
    the scroll target.** The segment wrapper is `id="dn33:1.11.0"` (full segment
    key); the reference anchor *inside* it is `id="1.11.0"`
    (`extract_short_reference()` takes the part **after** the colon,
    `helpers.rs:2184`) and exists only when `show_references` is on. Always
    target the full colon-bearing id, never the short one — it is the stable
    one, and the short form would also collide across columns in the
    multi-column layout.
13. **The fallback notice lives inside `#ssp_content`, which the find bar
    walks.** `find.ts` calls `findAndReplace(this.contentArea, …)` over
    everything under `#ssp_content` (`src-ts/find.ts:73`, `:357`), so a plain
    text node there would be spliced with `.ssp-find-highlight` spans and
    counted in the match total. **Requirements 26–27 accept that** in exchange
    for the message being selectable and copyable, which matters more: the
    reader's likely next move on seeing a bad location is to copy the sentence
    into an email. (A `data-` attribute rendered through CSS `content:` would
    hide the text from the find bar completely, but CSS-generated content is not
    selectable in Chromium — it was considered and rejected for exactly that
    reason. Do not reintroduce it.) Two things follow:
    - `findAndReplace` splices `.ssp-find-highlight` spans **into** the notice's
      text nodes when a search matches it. Attach the dismiss handler to the
      button (or delegate from the notice root), never to a captured text-node
      reference, so a search cannot break dismissal.
    - The "×" is a single character, below the find bar's 2-character minimum
      (`find.ts:324`), so the control itself can never be matched.
14. **Where the notice may be placed is constrained by the markup.**
    `<span class="segment">` is an inline span that the template wraps in a
    block (`template_str.replace("{}", &combined_segment)`), and in the
    multi-column layouts it is the grid container holding one
    `<span class='colcell col-N'>` per column (`helpers.rs:2426-2438`,
    `_suttacentral.sass:177-201`). Injecting a block element into it adds a
    phantom grid item and breaks the column alignment for that row — hence
    requirement 33's "sibling before the nearest block ancestor"
    (`el.closest('p, li, h1, h2, h3, blockquote') || el`). Verify in **Columns**
    layout specifically, not just Lines.
15. **Same uid *and* same anchor is a no-op.** The existing-tab update path
    (`SuttaSearchWindow.qml:1228`) rewrites `data_json`; QML fires
    `onData_jsonChanged` only when the string actually differs. Clicking a
    *different* location of an open sutta therefore reloads and scrolls
    correctly (requirement 17 — confirmed, the anchor is in the URL, §6.6), but
    clicking the **same** link again after scrolling away does nothing. Decide
    whether that is acceptable or whether *this* branch — same uid **and** same
    anchor, not same uid alone — should call `scroll_to_anchor()` directly. That
    is one line and removes a "the link is broken" report.
16. **Verified against the shipped database**: `dn33/pli/ms` `content_json`
    contains the key `dn33:1.11.0` (the screenshot's example), and so does
    `dn33/en/sujato`. But `dn33/pli/cst`, `dn33/en/thanissaro` and
    `dn33/en/tw-caf_rhysdavids` have **empty** `content_json` — these are the
    requirements 19–20 fallback cases, and they are reachable: they are ordinary
    translations a user may have open. Since `get_full_sutta_uid()` prefers
    `{uid}/pli/ms`, the Topic Index path lands on a segmented text in the normal
    case.
17. **`SuttaDisplayDefaults` gains a field, so old settings must still load.**
    This is already handled: the struct carries a **struct-level**
    `#[serde(default)]` (`app_settings.rs:468`), so simply adding the field and
    setting it in the `Default` impl is sufficient — a per-field
    `#[serde(default)]` is redundant and should not be added, because it implies
    the struct attribute is missing. (There is precedent for the three-site
    serde migration pattern in `docs/android-edge-to-edge-and-safe-areas.md` if
    a richer default is ever needed.) The same struct-level attribute is what
    makes the **persist path free**: `POST /save_sutta_display_settings`
    (`api.rs:1717`) deserializes the body straight into `SuttaDisplayDefaults`,
    and the client already sends its whole settings object — so no route, no
    handler and no payload shape changes for the new field.
    Note `sutta_display_js()` serializes the whole struct into the page
    as `defaults` (`app_data.rs:646-655`), so the new field appears there for
    free — and the panel's "Reset all" path reads it.
    `docs/sutta-display-settings-and-multi-column-view.md` must be updated with
    the new option, its scope semantics and the precedence rules of
    requirement 43.
18. `src-ts/display_settings.ts` has tests (`display_settings.test.ts`), and
    `content_reload.test.ts` already asserts on `show_references` in the
    content-block URL (`:26`, `:31-33`, `:141-149`) — extend both rather than
    adding an untested branch. Run `npx webpack` after editing TypeScript; the
    built bundle (`assets/js/simsapa.min.js`) is what the page loads.
19. **The new-window path goes through C++ and back into QML.**
    `open_sutta_search_window_with_result()` (`sutta_bridge.rs:3660`) →
    `callback_open_sutta_search_window` (`cpp/gui.cpp:210`) →
    `signal_open_sutta_search_window`. **Traced and confirmed**: the slot
    `WindowManager::open_sutta_search_window_with_query()`
    (`cpp/window_manager.cpp:497`) invokes the *same*
    `show_result_in_html_view_with_json` on the fresh window's root, so the
    `anchor` key needs no new plumbing at all.

    Requirement 16 still needs its own test in that mode, and for a sharper
    reason than "a different entry point": the fresh window takes the tab-0
    **update** branch (`SuttaSearchWindow.qml:1210-1240`), whose own comment
    records that the webview is not found the first time while the window
    objects are still being constructed. If `get_item()` returns nothing there,
    `data_json` — and with it the anchor — is dropped silently and the sutta
    opens at the top. This is the one place in the anchor path with a real risk
    of a silent loss.

### 6.1 `parse-cips-index` performance — measured, and what is worth changing

**The command is not slow and does not need optimising.** Measured on the real
inputs (21,790-row CSV, 3,202 headwords, shipped `appdata.sqlite3`), using the
**debug** binary at `cli/target/debug/simsapa_cli`:

| Invocation | Wall time |
|---|---|
| `--csv-path --json-path --db-path` (the normal one) | **0.95 s** |
| without `--db-path` | 0.85 s |
| with `--minify` | 0.80 s |

So the database title lookup costs ~0.10 s and pretty-printing ~0.05 s; the
remaining ~0.8 s is parse + sort + serialize, in an unoptimised build. A release
build is several times faster again. **Adding the anchor validation of §4.5 will
not be noticeable**: the segment keys of the 32 referenced suttas are a fraction
of the 36 MB / 0.12 s that loading *all* 7,285 blobs costs.

**Necessary, keep as is:**

- Pre-loading `HashMap<uid, title>` for all 7,288 pli/ms suttas
  (`cli/src/main.rs:797-813`). Only 3,998 uids are referenced, so ~45 % is
  unused — but it is one two-column query replacing 19,970 individual lookups.
  This is the right trade and must not be "optimised" into per-ref queries.
- `lazy_static!` for `RE_BOOK` / `RE_NUMERIC` (`:93-99`) — compiled once.
- Reading the CSV wholly into memory (2 MB) — fine.

**Genuinely wasteful, one-line fixes, worth doing while the file is open for the
suffix work:**

1. `headword_keys.sort_by_key(|a| get_headword_sort_key(a))`
   (`parse_cips_index.rs:419`) — `sort_by_key` calls the key function on **every
   comparison**, so ~37k invocations for 3,202 headwords instead of 3,202. And
   `get_headword_sort_key` is not cheap: `to_lowercase()`, a `String`-rebuilding
   strip loop, and `latinize()`. Use **`sort_by_cached_key`**.
2. `sub_keys.sort_by(… latinize(a).to_lowercase().cmp(&latinize(b).to_lowercase()))`
   (`:434`) — four allocations per comparison. Same fix.
3. `sorted_locators.sort_by(|a, b| compare_locators(a, b))` (`:446`) —
   `compare_locators` runs two regex scans (`extract_book` + `extract_numbers`,
   the latter allocating a `Vec<u32>`) **per operand per comparison**, plus a
   linear scan of `BOOK_ORDER`. Precompute `(book_order_index, Vec<u32>)` once
   per locator with `sort_by_cached_key`.

   **Equivalence checked, so this one is safe.** `compare_locators`
   (`:205-231`) is exactly that tuple compared lexicographically: its
   element-wise loop over the zipped number vectors followed by
   `nums_a.len().cmp(&nums_b.len())` ("shorter wins") *is* `Vec<u32>`'s derived
   `Ord`. Both `sort_by` and `sort_by_cached_key` are stable, so equal keys keep
   their input order either way. The tuple key reproduces the current order
   exactly — the caveat in task 1.8 about keeping `sort_by` if the tie-break is
   inexpressible does not apply.
4. `get_headword_sort_key`'s inner loop does `format!("{} ", word)` for each of
   the 13 `IGNORE_WORDS` on every iteration (`:163`) — allocating a `String` per
   check. Use a const array of already-spaced prefixes with `strip_prefix`.

**Cosmetic, mention only:** `validate_index` uses `HashMap<String, bool>`
(`:511`) where a `HashSet<String>` is meant.

None of these change behaviour, and none of them is why the command takes a
second. Do them because they are one-liners in a file being edited anyway — not
as a performance project, and **not** at the cost of touching the sort *order*,
which is what the generated JSON's stability depends on. Any such change must be
verified by regenerating the JSON and diffing it against the committed
`assets/general-index.json` — an empty diff (modulo the new `suffix` fields) is
the acceptance test.

## 7. Design Considerations

- Colon and gap only; **no** new separators, rules, background bands, or
  font changes for the sub-topic labels.
- The gap should be about one blank line at the current point size, scaled with
  `root.pointSize` so it holds on mobile (where `pointSize` is 16 rather than
  12) rather than being a hardcoded pixel value.
- The `(a)` suffix is plain text in the same style as the rest of the link
  label — it is part of the clickable link, not a separate superscript control.
- The anchor highlight colour should be visually distinct from the yellow/green
  search-match highlight (`root.match_bg`, `.ssp-find-highlight`) so a reader
  arriving from the Topic Index does not mistake it for a search hit.
- The reference numbers should match the reference screenshot: small, muted,
  set above/beside the paragraph, and never competing with the text. This is
  the existing `.reference` / `a.sc` styling — the work is exposing it, not
  restyling it. Check it in both Lines and Columns layouts, since the
  reference anchor is emitted from two separate call sites
  (`helpers.rs:2301` segmented, `:2421` multi-column).
- The **Show references** control sits under Repeat Pāli in the Layout card,
  before Width, so the two "what is shown per segment" controls are adjacent.
- The notice — in both its forms, fallback and give-up — should read as an
  **aside about the navigation**, not as
  part of the sutta: a full-width block with its own subdued background and a
  clear boundary, so no reader mistakes it for text of the discourse. The
  message is ordinary markup, so the two locations *may* be set in a distinct
  face (`<code>`) if that reads better — but it is optional; the block's own
  framing is what separates the notice from the discourse.
- The message must be **selectable and copyable**, since reporting a bad
  location by email is the expected follow-up. Check that a drag-select across
  the sentence yields the full text including both locations, and that the "×"
  does not land in the middle of the selection.
- Style it in `assets/sass/` next to the existing in-page chrome
  (`_find.scss`, `_confirm_modal.scss` are the nearest neighbours) and run
  `make sass`; never hand-edit `assets/css/`. It must be in normal page flow —
  **not** `position: fixed`, which would collide with the column bar and the
  footnote bottom bar, and on Android would put it in front of the native
  webview visibility machinery.
- The "×" needs a real hit area (≥ 44 px on mobile), an `aria-label`, and must
  be reachable by keyboard on desktop.

## 8. Success Metrics

1. Visual check against the example in §4.1: in the `A` letter section, each
   sub-topic label ends with a colon and is preceded by a clear gap; no gap is
   doubled under a headword.
2. `DN 33:1.11.0` no longer appears anywhere in the window; the link reads
   `DN 33 Saṅgītisutta`.
3. Clicking that link (both with and without "Open in new window") opens
   DN 33 scrolled to segment `dn33:1.11.0`, which is briefly highlighted.
4. Clicking a link whose reference has no segment id opens the sutta at the top,
   as before.
5. **Two named fallback checks**, using the locations the data actually
   contains:
   - *conditions (saṅkāra) → all beings sustained by* — note the headword is
     spelled without the **h** in the CSV; the Pāli is *saṅkhāra* and the entry
     is only findable by the misspelling until the source is corrected
     (see §C of the corrections doc) — (`dn33:1.7.9.1`, which
     does not exist) opens DN 33 scrolled to `dn33:1.7.9.0`, the heading
     *"1. Ones"*, with the notice **"Referenced location dn33:1.7.9.1 not
     found. This location dn33:1.7.9.0 is the closest fallback."** above it —
     and `1.7.9.0` printed beside the paragraph it landed on, matching the tail
     of the id in the notice. The notice is still on screen a minute later, and
     its "×" removes it.
   - *Māra → attacks gathering of arahants* (`dn20:4.15`) lands on
     `dn20:4.10` — five decrements — with the same notice shape.
   - The notice text can be **selected with the mouse and copied**, yielding the
     complete sentence with both full ids.
   - Running a find that matches a word in the notice highlights it like any
     other text and the dismiss "×" still works afterwards.
6. A link whose segment is absent **and** whose parent section offers nothing
   (a legacy text with no segments at all) opens the sutta at the top with the
   notice **"Referenced location dn20:4.11 not found."** — one sentence, no
   fallback clause — as the first thing above the sutta title, dismissible the
   same way. There is no error dialog, and one `logger.warn()` line is written.

   **How to reach this state**, since no Topic Index link produces it directly:
   every segment-carrying ref resolves against `{uid}/pli/ms`, which is
   segmented in all 32 referenced suttas. Open the link, then switch the reading
   panel to a **non-segmented** text of the same sutta — `dn33/pli/cst`,
   `dn33/en/thanissaro` and `dn33/en/tw-caf_rhysdavids` all have an empty
   `content_json` (§6.16, verified) — and jump again.
7. A notice appears **only** when something went wrong: an exact hit shows none,
   and a reference with no location at all shows none. Two misses in a row leave
   exactly one notice, and an exact hit after a miss leaves none.
8. In **Columns** layout, a fallback notice does not disturb the alignment of
   the column cells in the row it precedes.
9. A named check: under headword **feet**, sub-topic **Buddhas'**, the five
   `dn30` refs show `(a)` through `(e)` and each jumps to its own paragraph
   (`dn30:1.4.0`, `1.7.0`, `1.10.0`, `1.16.0`, `1.19.0`) — with the letters
   present in the **generated JSON**, not computed in QML. The regenerated
   index must contain exactly **55 suffixed refs across 25 groups**; a
   different count means the scoping rule (requirement 12) was implemented
   differently.
10. Opening a Topic Index link shows the reference numbers (requirement 43,
   rule 2) even though the persisted default is off, and the number beside the
   highlighted paragraph is the one the index cited. Switching references off in
   the cogwheel on that page hides them and the page still works; a subsequent
   Layout change does not bring them back.
11. Turning **Show references** on with "Save as default" survives an app
   restart, applies to a sutta opened *without* an anchor, and works in both
   Lines and Columns layouts.
12. Re-running `parse-cips-index` against the current CSV prints
   `1579 checked, 1573 ok, 0 unresolved uid, 0 no segments, 6 missing segment`
   (or the same numbers modulo genuine CSV changes), names all six failing
   locations, and still writes the JSON and exits 0.
13. `make qml-test` passes; `qmllint` reports no new warnings for
    `TopicIndexWindow.qml`; `cd backend && cargo test`, the CLI's own tests and
    the Jest suites pass, including new cases for suffix computation and for the
    new display setting.

## 9. Open Questions

1. Should the paragraph highlight also apply when the user *returns* to the tab
   later (e.g. via navigation history), or strictly once at open time? Assumed:
   once at open time.
2. **The DN 20 `4.x` block needs the index author's attention, and the fallback
   will hide it.** All six failing locations are rescued by requirement 19 —
   `dn33:1.7.9.1` lands on `1.7.9.0` (*"1. Ones"*, the right place) and
   `dn20:4.11`–`4.15` land on `dn20:4.10`. But the review in
   [2026-08-11-144238-cips-paragraph-location-corrections.md](./2026-08-11-144238-cips-paragraph-location-corrections.md)
   found that **ten further DN 20 locations resolve successfully to the wrong
   text** (`dn20:4.2`, cited for *gandhabbas*, is *"most of the deities from ten
   solar systems have gathered…"*). No fallback can detect that. Does the CSV
   get corrected before or after this feature ships?

   The same question covers §C of that document — the headword
   `conditions (saṅkāra)`, misspelled for *saṅkhāra* on 10 CSV rows. It is
   unrelated to the locations, but it is a one-word source fix in the same file,
   and correcting it changes the string that §8.5 and the validation warnings
   name.
3. On mobile, is `scroll_to_anchor()` reliable through the native `QtWebView`
   for a colon-bearing id, or does it need the URL-fragment path only?
   **Partly answered:** the *callback* form `runJavaScript(script, fn)` — the
   part §6.7 and requirement 22 depend on — is already used unconditionally on
   both platforms (`SuttaSearchWindow.qml:147`,
   `get_current_scroll_position()`), so there is no platform branch to write for
   the logging. What remains open is only whether the scroll itself lands
   reliably for a colon-bearing id on the native webview.
4. Should clicking the **same** link twice re-scroll (§6.15)? Assumed no change
   is a defect worth one line of code, but it is a behaviour choice. Note this
   is narrower than it first appears: a *different* location of the same open
   sutta already re-scrolls today, because the anchor is part of the URL (§6.6).
5. Requirement 45's scope limit: clear `root.anchor` after a resolved jump, so
   a later full reload of that tab does not re-force the references on, or
   accept that it does?
5. Does the **Show references** option need to apply to book chapters and other
   non-sutta content shown in the same panel, or is it sutta-only? Assumed:
   sutta-only, since the reference numbers come from Bilara segment ids.
