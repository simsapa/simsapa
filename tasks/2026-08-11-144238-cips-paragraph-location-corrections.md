# CIPS topic index — paragraph location review

Prepared 2026-08-11 for review by the topic index author; updated 2026-08-12,
when the paragraph-link feature shipped, to separate the defects the tooling
can report from the ones only a human reading the passage can catch.

Companion to
[2026-08-11-144238-prd---topic-index-list-readability-and-paragraph-links.md](./2026-08-11-144238-prd---topic-index-list-readability-and-paragraph-links.md).

## What was checked

Every locator in `general-index.csv` that carries a paragraph location (the part
after the colon, e.g. `DN33:1.11.0`) was checked against the actual SuttaCentral
Bilara segment ids of the Pāli text (`{uid}/pli/ms`), which is the text the app
opens when a topic link is clicked.

| | |
|---|---|
| Sutta references in the index | 19,970 |
| …of which carry a paragraph location | 1,579 (7.9 %) — all in **DN** |
| Distinct suttas involved | 32 |
| Locations that resolve to the intended passage | **1,563** |
| Locations that resolve, but to the **wrong** passage | **10** — §B |
| Locations that do not exist in the text | **6** — §A, §B |

**The 10 in the middle row are the ones that need you most**, and they are the
reason this document exists rather than just a warning in the build log. They
are all DN 20 `4.1`–`4.10`. The automated check can only ask *"does this
segment id exist?"*, and for these the answer is yes — so they pass validation
silently, and the app's fallback never fires, because there is nothing for it
to detect. A reader clicking *gandhabbas, visiting monastics* today is taken,
with no warning of any kind, to a paragraph about deities gathering.

The 6 missing ones are the *safer* failure: the tooling reports them, and the
reader gets an explicit notice naming the location that was missing (see
[What the app does meanwhile](#what-the-app-does-meanwhile)).

Only a correction to `general-index.csv` can fix either class. The pipeline
reports source-data defects and never repairs them, so nothing changes until
the source is edited.

## The convention the data follows

99 % of the locations point at a **section heading** segment, and the index is
internally very consistent about this:

- 1,251 end in `.0` — e.g. `DN33:1.7.9.0` = *"1. Ones"*, `DN20:4.0` =
  *"1. The Gathering of Deities"*.
- 313 end in `.0.1` — the same thing where headings are nested, e.g.
  `DN2:34.0.1` = *"4. The Fruits of the Ascetic Life"*,
  `DN22:13.0.1` = *"4. Observing Principles"*, `DN1:1.7.0.1` = *"2. Ethics"*.

Only **16 locations in two suttas** depart from that convention, and they are
exactly where the problems are. Everything else checks out.

Sections **C** and **D** are unrelated to the locations — a headword spelling and
five duplicated cross-reference rows, both found while working on the index and
recorded here because they are one-line source edits in the same file.

## A. DN 33 — one location, off by one row

`DN33:1.7.9.0` is the heading *"1. Ones"*. The adjacent row appends `.1` to that
heading id, as if the items under the heading were numbered `.1`, `.2`, … They
are not — the heading's siblings continue as `1.7.10`, `1.8.1`, `1.8.2`, …

| Original | Exists? | Currently points at | Suggested | Comment |
|---|---|---|---|---|
| `DN33:1.7.9.0`<br>*food — all beings sustained by* | ✅ | *"1. Ones"* (heading) | `dn33:1.8.2` | Resolves, but to the section heading. `1.8.2` is the sentence itself: *"All sentient beings are sustained by food."* Keep the heading if section-level is intended. |
| `DN33:1.7.9.1`<br>*conditions (saṅkāra) — all beings sustained by*<br>(the headword's spelling is itself a typo — see §C) | ❌ | — | `dn33:1.8.3` | Does not exist. `1.8.3` is *"All sentient beings are sustained by conditions."* If section-level is preferred, use `1.7.9.0` like its sibling row above. |

## B. DN 20 — the whole `4.x` block

**All 15 rows in this section are wrong, but in two different ways, and only
the second way is visible to a reader.** `4.1`–`4.10` exist and therefore
resolve, silently, to unrelated prose — no warning, no notice, nothing in the
build log. `4.11`–`4.15` do not exist, so the tooling reports them and the
reader gets a notice. The first ten are the harder problem precisely because
nothing flags them.

DN 20 has only **one** heading segment in the entire sutta — `dn20:4.0`,
*"1. The Gathering of Deities"* — so the heading convention offers nothing
finer, and the index instead numbers the arrivals itself as `4.1` … `4.15`.
Those ids do exist in the text up to `4.10`, but they are the **prose
introduction**, not the passages meant: `dn20:4.2` is *"Mendicants, most of the
deities from ten solar systems have gathered…"*, not the gandhabbas.
`4.11`–`4.15` run off the end of the section entirely.

The actual passages are in the verse sections that follow (5 – 22). Suggested
targets below are keyword matches against the Pāli and Ven. Sujato's
translation, and need your confirmation.

| Original | Exists? | Currently points at | Suggested | Comment |
|---|---|---|---|---|
| `DN20:4.1` — *Four Great Kings, described* | ✅ | *"Then the Buddha said to the mendicants:"* | `dn20:9.41` | `9.41`–`9.44` name the four kings by direction; `9.1` begins *"King Dhataraṭṭha rules…"* |
| `DN20:4.2` — *gandhabbas, visiting monastics* | ✅ | *"…deities from ten solar systems have gathered…"* | `dn20:10.9` | *"Cittasena the centaur came too"*; `9.3` is *"Lord of the centaurs"* |
| `DN20:4.3` — *nāgas, visiting monastics* | ✅ | *"Those who were perfected ones… in the past"* | `dn20:11.1` | *"Then came the dragons of Nābhasa lake"* |
| `DN20:4.4` — *supaṇṇas, visiting monastics* | ✅ | *"Those who will be perfected ones… in the future"* | `dn20:11.12` | *"their name is 'Rainbow Phoenix'"* |
| `DN20:4.5` — *nāgas, going for refuge* | ✅ | *"I shall declare the names of the heavenly hosts"* | `dn20:11.16` | *"the dragons and phoenixes took the Buddha as refuge"* — same segment as the next row |
| `DN20:4.6` — *supaṇṇas, going for refuge* | ✅ | *"I shall extol the names…"* | `dn20:11.16` | as above; the two are one line in the text |
| `DN20:4.7` — *yakkhas, visiting monastics* | ✅ | *"I shall teach the names…"* | `dn20:7.1` | *"There are seven thousand spirits"* — note this is **earlier** than the rows above it, so the index's `4.x` order does not follow the sutta |
| `DN20:4.8` — *Varuṇa, visiting monastics* | ✅ | *"Listen and apply your mind well…"* | `dn20:13.3` | *"The gods of Varuṇa and Varuṇa's offspring"* |
| `DN20:4.9` — *devas, visiting monastics* | ✅ | *"'Yes, sir,' they replied."* | — | Too general to place; which passage was meant? |
| `DN20:4.10` — *Varuṇa, visiting monastics* | ✅ | *"The Buddha said this:"* | `dn20:15.5` | *"The Varuṇas and Sahadhammas"* — a second Varuṇa mention, which may be why Varuṇa appears twice (`4.8` and `4.10`) |
| `DN20:4.11` — *gods of Yama, visiting monastics* | ❌ | — | `dn20:18.1` | *"The Khemiyas from the realms of Tusita and Yama"* |
| `DN20:4.12` — *gods that delight in creation* | ❌ | — | `dn20:18.5` | *"The gods who love to create came too"* |
| `DN20:4.13` — *gods that delight in creation* | ❌ | — | `dn20:18.5` | Three consecutive rows carry the same heading; is one segment intended, or three different ones? |
| `DN20:4.14` — *gods that delight in creation* | ❌ | — | `dn20:18.5` | as above |
| `DN20:4.15` — *Māra, attacks gathering of arahants* | ❌ | — | `dn20:21.3` | *"Māra's army came forth too"*; `22.5` is *"Māra's army has arrived"* |

## C. A headword spelling, unrelated to the locations

Separate from the paragraph-location review, and noticed while cross-checking
the headwords named above: the headword

> `conditions (saṅkāra)`

is missing its **h** — the Pāli is *saṅkhāra*. It appears on **10 rows** of
`general-index.csv` (lines 2589, 2649, 2670, 3392, 4591, 7036, 7039, 8740,
16531, 20030), always with the same spelling, so it is one typo repeated rather
than an inconsistency within the entry.

The rest of the file has it right — `formations (saṅkhāra)`,
`volitional formations (saṅkhāra)`, `bodily formation (kāyasaṅkhāra)`,
`suffering due to formations, conditions (saṅkhāradukkha)` and twenty more all
spell it correctly. There is no existing `conditions (saṅkhāra)` headword, so
correcting these ten rows renames the entry rather than merging two.

Nothing in the app or the tooling will change this on its own — the pipeline
reports source-data defects and never repairs them (PRD §4.6), so the entry will
keep its current spelling until the CSV is edited.

Two consequences worth knowing before it is corrected:

- The headword text is what a reader types into the Topic Index search box, so
  today the entry is only findable by the misspelling.
- `headword_id` is derived from the headword text (`make_normalized_id()`), so
  the id changes with the spelling. Nothing persists headword ids across runs,
  so this is safe — but any note or test that names the id (or the headword) has
  to be updated in the same pass.

## D. Five duplicated cross-reference rows

Found separately from the location review, while making the generated index
byte-reproducible: five cross-references are entered **twice**, as fully
identical rows (same headword, same empty sub-topic, same target).

| Rows | Headword | Cross-reference |
|---|---|---|
| 216, 221 | `bhikkhunīs` | `xref Āḷavikā, Ven.` |
| 220, 232 | `bhikkhunīs` | `xref Jaṭilagāhikā, Ven.` |
| 224, 234 | `bhikkhunīs` | `xref Nandā, Ven.` |
| 515, 516 | `occupations` | `xref hunters` |
| 1609, 1610 | `craving (taṇhā)` | `xref craving to end existence (vibhavataṇhā)` |

Two of the pairs are adjacent rows (515/516, 1609/1610), which looks like a
duplicated line while editing. The three `bhikkhunīs` pairs are separated by
several rows, so they look more like the same name being added twice to a long
list.

The same check over the **sutta-locator** rows found **0** duplicates, so this
is confined to cross-references.

Nothing in the pipeline removes them: the parser reports source-data defects and
never repairs them (PRD §4.6), and de-duplication was explicitly rejected for
sutta references. Deleting one row of each pair is a source edit for the author.

Until then, each duplicate shows in the app as the same `• see: …` line twice
under its headword. One small thing has changed in their favour: cross-references
are now sorted rather than emitted in CSV row order, so a duplicated pair always
appears **adjacent**, which makes it obvious rather than scattered down a list.

## Other

Also worth confirming: `DN20:4.0` is used by six rows (*non-humans*, *earth
gods*, and the four kings by direction). It resolves to the sutta's only
heading, so those six all land in the same place. The four direction rows would
be better served by `dn20:9.41`–`9.44`, and *earth gods* by `dn20:7.2`
(*"earth-gods of Kapilavatthu"*).

## What the app does meanwhile

This is implemented and shipped, so it describes current behaviour rather than
a plan.

A location that does not exist does not leave the reader stranded. The app
walks back to the nearest preceding paragraph **within the same parent
section**, and only that far — so `1.7.9.1` lands on `1.7.9.0` (*"1. Ones"* —
the right place), and `DN20:4.11`–`4.15` land on `dn20:4.10`, which is at least
inside the Gathering of Deities. Either way a dismissible notice at the landing
place names the location that was missing and the one used instead, in full
(`dn20:4.11`), so it can be copied into a reply to this document. If nothing in
the parent matches, the sutta opens at the top with a notice saying only that
the referenced location was not found, and the reader can search for the
passage.

The walk deliberately stops at the parent and never continues to a sibling of
it: a sibling may be an entirely different chapter, and landing there silently
would be worse than landing at the top.

**That fallback is a safety net, not a fix, and it is blind to the larger half
of §B.** It cannot know that `DN20:4.2` was meant to be the gandhabbas,
because `4.2` exists and points somewhere else — so those ten rows produce no
notice, no warning and no log line. They look correct from every angle except
reading the paragraph. Only a correction to the source data reaches them.

## What we are asking for

Four independent decisions, in rough order of how much they affect readers.
None of them is urgent, and nothing in the app is blocked on them — the
feature ships and works with the data as it stands.

1. **§B, DN 20 `4.1`–`4.10` (10 rows).** The most valuable to fix, because
   they are wrong *and* invisible. The suggested targets are keyword matches
   against the Pāli and Ven. Sujato's translation and need your confirmation —
   we do not want to guess at what a topic entry was meant to point to. Two
   specific questions inside that table: `4.9` (*devas, visiting monastics*)
   is too general for us to place at all; and `4.12`–`4.14` are three
   consecutive rows carrying the same heading — is one segment intended, or
   three different ones?
2. **§B, DN 20 `4.11`–`4.15` (5 rows) and §A, DN 33 `1.7.9.1` (1 row).** The
   six that do not exist. Readers do get a notice here, so the harm is smaller,
   but the notice is an apology rather than a destination.
3. **§C, the `conditions (saṅkāra)` headword** (10 rows, missing *h*). A
   single find-and-replace. Worth knowing: the entry is currently findable in
   the app's Topic Index search **only** by the misspelling.
4. **§D, five duplicated cross-reference rows.** Delete one row of each pair.
   Cosmetic — they show as the same `• see:` line twice.

Also flagged, not defects: the six rows all pointing at `DN20:4.0`, and
`DN33:1.7.9.0` pointing at a heading rather than the sentence beneath it (see
§A and "Other"). Both resolve to a reasonable place; they are listed in case
finer targets were intended.

A reply naming row numbers and replacement locators is enough — we make the
CSV edit, regenerate the index and re-run the validation. Re-running it is how
we confirm a correction landed: the summary line currently reads
`1579 checked, 1573 ok, 0 unresolved uid, 0 no segments, 6 missing segment`,
and items 1 and 2 above would move it to `1579 ok`. Note that the count alone
cannot confirm item 1 — those rows already count as `ok` today. Only reading
the passages can.
