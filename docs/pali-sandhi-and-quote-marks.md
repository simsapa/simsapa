# Pāli sandhi break-down and quote marks

How the app takes apart Pāli words joined by sandhi, above all the **iti
sandhi** (`dhārayāmī’ti` = *dhārayāmi* + *iti*), and how it handles the single
and double quote marks, smart and straight, that the editions write inside and
around those words. It covers every place that depends on it: stored search
text, search queries, the dictionary lookups, glossing, and the reader page's
tap, double-click and find bar.

The details of each feature live in its own doc; this one is the map, the
shared rules, and the decisions that tie them together:

- Gloss word extraction and position finding:
  [glossing-and-extracting-words-with-context-algorithm.md](./glossing-and-extracting-words-with-context-algorithm.md)
- Gloss context hash and the compound break-down selector:
  [gloss-ai-word-selection.md](./gloss-ai-word-selection.md) (§ cache key, §9)
- The Dictionary result page and the grouped DPD lookup:
  [search-snippet-highlight-pipeline.md](./search-snippet-highlight-pipeline.md) §9
- The stored text normalization at bootstrap:
  [text-processing-for-contains-match-and-fulltext-match-search.md](./text-processing-for-contains-match-and-fulltext-match-search.md)

---

## 1. What is being taken apart

**Iti sandhi.** A quotation closes with *iti* ("thus"), which fuses with the
word before it. The final vowel lengthens and a final *ṁ* becomes *n*:

| parts | sandhi | how editions write it |
|---|---|---|
| dhārayāmi + iti | dhārayāmīti | `dhārayāmī’ti`, `dhārayāmī”ti`, `dhārayāmī’”ti`, `dhārayāmī"ti`, `dhārayāmī'"ti`, `dhārayāmīti` |
| viharissāma + iti | viharissāmāti | `viharissāmā’ti` |
| sādhu + iti | sādhūti | `sādhū’ti` |
| gantuṁ + iti | gantunti | `gantun’ti`, `gantu’nti`, `gantun”ti`, `gantunti` |
| maggaṁ + iti | magganti | `magga’’nti` (CST) |

The quote marks are the edition marking where the quoted speech ends, often
nested (`’”` closes a quote inside a quote). The two text sources write them
differently, which is why every quote style has to work:

- **SuttaCentral** (`/ms`) mostly writes `”ti`, then `’ti`, with `’”ti` and
  `”’ti` for nested speech.
- **CST** (`/cst`) mostly writes `’’ti`, then `’ti`, rarely `’’’ti`, the
  opening `‘‘ti` (a source error) and straight `'nti`.

**Compound sandhi.** Words also fuse inside compounds (`pañcaggadāyakaṁ` =
pañca + agga + dāyakaṁ). The app does not split these itself. The DPD
deconstructor (the `lookup` table in `dpd.sqlite3`) lists one or more
**break-downs** per word, and the grouped DPD lookup turns each into components
with their own results. Iti sandhi meets this at one point: DPD's break-downs
for an iti-sandhi word end in `+ iti`, and the lookup relies on that (§4).

---

## 2. The quote characters, and who recognises which

| char | name | code point |
|---|---|---|
| `'` | apostrophe (straight single) | U+0027 |
| `"` | quotation mark (straight double) | U+0022 |
| `‘` | left single quotation mark | U+2018 |
| `’` | right single quotation mark (also the typographic apostrophe) | U+2019 |
| `“` | left double quotation mark | U+201C |
| `”` | right double quotation mark | U+201D |
| `ʼ` | modifier letter apostrophe | U+02BC |

Each component recognises its own set, and the differences matter:

| component | `'` | `"` | `‘` | `’` | `“` | `”` | `ʼ` | what it does with them |
|---|---|---|---|---|---|---|---|---|
| `normalize_iti_sandhi` (`RE_*TI_*`) | ✓ | ✓ | | ✓ | | ✓ | | splits `<vowel><quotes>ti` (§3) |
| `remove_punct`, `RE_MID_WORD_STRAIGHT_QUOTE` | ✓ | ✓ | | | | | | removes it between two letters, joining the word |
| `remove_punct`, `RE_PUNCT_QUOTES` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | | turns it into a space |
| `split_possessive_apostrophe` (+ backtick) | ✓ | | ✓ | ✓ | | | ✓ | `day's` → `day s` |
| `normalize_fulltext_query` strip (+ backtick) | ✓ | | ✓ | ✓ | | | ✓ | deletes it; double quotes stay for tantivy |
| `join_mid_word_apostrophes`, before a word-final `ti` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | turns the run into a space: still iti (§4) |
| `join_mid_word_apostrophes`, before `ti`/`nti` + more letters | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | removes the run, joining the word |
| `join_mid_word_apostrophes`, one between two letters | ✓ | | ✓ | ✓ | | | ✓ | removes it, joining the word |
| gloss `is_quote_char` (+ `ʼ` in the inner-quote match) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | | skips quote runs when finding a word's position |
| gloss `RE_GLOSS_PUNCT` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | | turns it into a space |
| JS `pali_word.ts`, inside a word | ✓ | | | ✓ | | | | part of the word |
| JS `pali_word.ts`, before `ti` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | | the word continues into `ti` / `nti` and any letters after (§6) |
| JS `find.ts` inter-word class | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | | matches between two find words |
| JS `find.ts` before `ti` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | | zero or more, so the joined form matches (§6) |
| browser word selection | joins | splits | joins | joins | splits | splits | joins | Unicode word boundaries (UAX #29), for one mark between two letters |

Two asymmetries are deliberate:

- **In search, a straight quote between letters joins the word; a smart one
  splits it.** `remove_punct` gives `manopubbaṅ'gamā` → `manopubbaṅgamā` but
  `manopubbaṅ’gamā` → `manopubbaṅ gamā`. In running text a smart `’` or `”` is
  more often the end of quoted speech than a mark inside a compound, and since
  the stored text and the query take the same step, Contains search still
  matches. The Fulltext query deletes single apostrophes instead, and the DPD
  lookups and the Gloss tab join them (`join_mid_word_apostrophes`), because
  they look up one word: `manopubbaṅ’gamā` finds *manopubbaṅgama*. It is not
  iti sandhi, and no iti rule touches it.
- **The iti regexes take only closing and straight quotes.** An opening `‘` or
  `“` before `ti` is a source error. It still ends up as `<word> ti`, because
  `remove_punct` turns it into a space, but without the vowel shortening.

---

## 3. The shared normalization: splitting the iti sandhi

`normalize_iti_sandhi()` (`backend/src/helpers.rs`) splits an iti-sandhi word
into `<base> ti`, restoring the base word's own ending. **Every rule matches
only at the end of a word** (`\b` after `ti`):

| rule | example |
|---|---|
| `ī` + quotes + `ti`, or quotes + `īti` → `i ti` | `dhārayāmī’”ti` → `dhārayāmi ti` |
| `mīti` at a word end → `mi ti` (no quote needed) | `dhārayāmīti` → `dhārayāmi ti` |
| `ā` + quotes + `ti`, or quotes + `āti` → `a ti` | `viharissāmā’ti` → `viharissāma ti` |
| `ū` + quotes + `ti`, or quotes + `ūti` → `u ti` | `sikkhāpadesū’ti` → `sikkhāpadesu ti` |
| `bhikkhūti` at a word end → `bhikkhu ti` | |
| `n` + quotes + `ti`, or quotes + `nti` → `ṁ ti` | `gantun’ti`, `gantu’nti` → `gantuṁ ti` |
| `unti` at a word end → `uṁ ti` (no quote needed) | `gantunti` → `gantuṁ ti` |

What it leaves alone, on purpose:

- **Unquoted `-āti`, `-īti`, `-ūti`, `-anti`, `-enti`.** These are ordinary verb
  and noun endings (`gacchati`, `gacchanti`, `pīti`), so without a quote there is
  no telling them from sandhi. The two unquoted rules, `mīti` and `unti`, exist
  because no verb ending looks like them.
- **Other final vowels** (`kho’ti`, `bhante”ti`). No vowel changes in sandhi
  there; the quote becomes a space later and gives `kho ti`.
- **Anything inside a word.** Without a word end the same letters are
  something else: English `until`, `counting`, `mounting`; Pāli `kuntini` and
  the compound `anaññātaññassāmītindriyaṁ`, which DPD knows whole. A quoted `ti`
  with more letters after it is iti fused with the next word, *pi* or *assa*:
  `maraṇā’tipi`, `cā”tipi`, `hevan’tissa`, `vadāmītipi`. DPD has those joined,
  with break-downs such as `maraṇā + iti + api`, so the steps that need one word
  join them (§4, §5) instead of splitting them.

**Trap: it must run before quote marks become spaces.** It needs the quote to
know the word is sandhi. `normalize_plain_text()` calls it first, and
`remove_punct()` runs afterwards.

**Trap: a change to these rules changes the stored text.** `content_plain`, the
FTS5 tables and the Tantivy indexes hold the output of these rules from the
time they were built. Queries take the rules of the running app, so after any
change to them the databases must be **re-bootstrapped** and shipped together
with the app, or a search for an affected word misses where the stored copy
still has the old form.

### Where the split applies

`normalize_plain_text()` (lowercase, `consistent_niggahita`,
`normalize_iti_sandhi`, space collapse) is the base of every Pāli text
normalizer, so the split reaches:

| path | function | used for |
|---|---|---|
| stored text | `compact_rich_text` → `compact_plain_text` | `content_plain` of suttas, dictionary definitions, imported books (bootstrap and import time) |
| Contains / Title / Headword / DPD queries | `normalize_query_text` → `compact_plain_text` | `SearchQueryTask::new` for every mode except UidMatch and FulltextMatch |
| Fulltext queries | `normalize_fulltext_query` | keeps tantivy's `"`, `+`, `-` operators |
| gloss word extraction | `preprocess_text_for_word_extraction` | the words the Gloss tab looks up |
| gloss context hash | `normalize_gloss_context` | the AI word-selection cache key |
| DPD lookups | `normalize_dpd_query_text` | rejoins the split again (§4) |

**The rule behind it: the stored text and the query go through the same
function**, so the quote style of either side does not matter. `dhārayāmī”ti`
typed into a Contains search finds `dhārayāmī’ti` in the text, because both
become `dhārayāmi ti`.

---

## 4. Dictionary lookups: rejoining, and when not to

DPD's `lookup` table is keyed by the **joined** sandhi form (`dhārayāmīti`,
`gantunti`, `viharissāmāti`) and has no row for the two words `dhārayāmi ti`. So
the DPD lookups undo the split before looking anything up.

**`normalize_dpd_query_text(text) -> (query, iti_base)`** is
`normalize_dpd_query_split()` followed by `rejoin_iti_sandhi()`.
`dpd_lookup()`, `dpd_lookup_grouped()` and `dpd_deconstructor_query()` all
normalize with it.

`normalize_dpd_query_split()` gives the split form, in this order:

1. `normalize_plain_text`: the iti split of §3, which takes the quotes it needs.
2. `join_mid_word_apostrophes`:
   - a quote run before a word-final `ti` becomes a space: `kho’ti` → `kho ti`,
     `bhante”ti` → `bhante ti`. These are iti too, for vowels the §3 rules do
     not cover;
   - a quote run before `ti`/`nti` with more letters after it is removed:
     `cā”tipi` → `cātipi` (ca + iti + api);
   - a single `’`, `‘`, `ʼ` or `'` between two letters is removed:
     `manopubbaṅ’gamā` → `manopubbaṅgamā`, `so’pi` → `sopi`.
3. `normalize_query_text`.

`rejoin_iti_sandhi()` then turns a single `<base> ti` into the joined key:
lengthen a final a / i / u, turn a final ṁ into `n`, append `ti`. It returns the
joined form and the base word.

**Trap: the order is load-bearing.** Joining before the iti split, or before a
word-final quote run becomes a space, would turn `magga’nti` into `magganti` and
`kho’ti` into `khoti` with the quote gone, so the lookup could no longer tell
them from a plain DPD key, and `magganti` is a verb form.

**Rule: any new DPD query normalizes with `normalize_dpd_query_text()`, never
`normalize_query_text()` alone.**

**A caller that hands the query on passes the split form.** `query_task.rs`
keeps it as `SearchQueryTask::dpd_query_text`, and
`SuttaBridge::results_page()` computes it for the break-down selector; both go
through `dpd_lookup_grouped_memo()` with the same string. Normalizing the split
form again gives the same split form, so the lookup reaches the same answer as
the word summary passing the raw word. Never pass on:

- `normalize_query_text`'s output: it has already turned `manopubbaṅ’gamā` into
  two words;
- the rejoined form: `magganti` normalized again is a plain word, the verb, not
  maggaṁ + iti.

### The joined form is used only when DPD calls it iti sandhi

A joined key can be an unrelated word. `magga’’nti` is *maggaṁ* + *iti*, but
DPD's `magganti` is only the plural of the verb *maggati*. The quote in the
source is the evidence that this is sandhi, so a verb form must never be the
answer.

So the lookups check **`dpd_has_iti_breakdown(joined)`**: whether one of the
break-downs of the joined key's `lookup` row ends in `iti`.

- **Yes** (`dhārayāmīti` → `dhārayāmi + iti`): the normal lookup of the joined
  form. DPD's own analysis, direct matches and break-downs included.
- **No** (DPD lacks the joined key, or has it as another word): the word is
  **`<base> + iti`**, built by `dpd_lookup_grouped_iti_split()`:
  - the base word: the shortened base (`dhārayāmi`), or, when DPD has no
    results for it, the unshortened one, because a long vowel can be a real
    ending (`anaññatā’ti` is *anaññatā* + *iti*). A final `n` before `ti` is
    always the `ṁ` form, so it is never kept as `n`;
  - results: the base word's results, then the `iti` component's;
  - break-downs: `<base> + iti` over the base word's direct matches, then each of
    the base word's own break-downs with `+ iti` appended. A base word with
    neither gets the single `<base> + iti` over whatever it returned;
  - no direct uids, so the break-down lock filters by the base analysis.

  The flat `dpd_lookup()` returns the same rows, and `dpd_deconstructor_list()`
  the same break-downs.

Unquoted `magganti` is not split by the normalization, so it never reaches this
path and stays the verb.

**No prefix guesses for an iti-sandhi word.** The deconstructor matches only
the exact joined key, and the base word's break-downs are exact only. A "starts
with" guess belongs to some other word (`vuccaṁ` matched `vuccati + iti`, which
would have given `vuccati + iti + iti`). How the guesses work for other words:
[search-snippet-highlight-pipeline.md §9e](./search-snippet-highlight-pipeline.md).

The `iti` component resolves to the one `iti` headword DPD uses in its own
break-downs, which is narrower than a lookup of the word `iti`.

---

## 5. Glossing

The Gloss tab works on the split form and **looks up the base word**, never the
joined iti form:

- `preprocess_text_for_word_extraction` splits the text (§3), runs
  `join_mid_word_apostrophes` (§4 step 2), and turns every remaining non-word
  character into a space. So `dhārayāmī’ti` yields the words `dhārayāmi` and
  `ti`, while `manopubbaṅ’gamā`, `so’pi` and `cā”tipi` stay one word each
  (`manopubbaṅgamā`, `sopi`, `cātipi`). Most elided forms are DPD keys only when
  joined, and DPD's break-down then gives the parts (`so + api`).
- `extract_words_with_context` finds each word's position in the original text
  with sandhi-aware matching (vowel length, `ṁ`/`n`, quote runs via
  `is_quote_char`), so the word as written is the whole `dhārayāmī’ti`. A joined
  word is found by `match_with_inner_apostrophes`, which skips quote marks
  between two letters of the original (`Manopubbaṅ’gamā` for `manopubbaṅgamā`).
  The same matcher skips the hyphen of a compound that
  `remove_inter_word_hyphens` joined (`Todeyya-kappā` for `todeyyakappā`), and a
  joined word followed by a quoted iti is the whole `maraṇā’tipī”ti`.
  It then **skips the standalone `ti`** after a word that ended in quote(s) +
  `ti`/`nti`: that `ti` is the *iti* of the sandhi, already part of the previous
  word.
- §3 splits an **unquoted** iti too (`viharāmīti` → `viharāmi ti`,
  `abhinandunti.` → `abhinanduṁ ti`). When the next clean word is that `ti`,
  `find_word_position` also matches the word joined to a bare `ti`, and the
  `ti` is skipped as above. Only with that `ti` next: otherwise `pi` would match
  inside `pīti`. **Trap:** a clean word the search cannot place at its spot is
  found at its *next* occurrence further on, and every word in between then
  fails to be placed (no `<b>` in its context, `original_word` = the clean
  word), to the end of the paragraph. One unmatched spelling therefore costs a
  whole paragraph, not one word. `test_extract_words_with_context.rs` asserts
  that every token of a real passage is placed.
- Each word is looked up with `dpd_lookup_grouped(word)`, so `magga’’nti` glosses
  as *maggaṁ* → *magga*, consistent with §4. When the clean word finds nothing
  and the text writes its final vowel long before the quote run,
  `gloss_lookup_words` retries the long form (`anaññatā’ti`: *anaññata* is not
  in DPD, *anaññatā* is). `WordInfo.original_word` carries the word as written
  for this; the clean word stays the word's identity, its cache key and
  display.

The **context hash** (`normalize_gloss_context`) runs the same split and then
rejoins `ṁ ti` → `nti`, so `cittan”ti`, `cittan'ti` and `cittanti` share one
cache key. That rejoin is for the hash only and does not touch search.

Details: [glossing-and-extracting-words-with-context-algorithm.md](./glossing-and-extracting-words-with-context-algorithm.md),
[gloss-ai-word-selection.md](./gloss-ai-word-selection.md).

---

## 6. The reader page (JS)

### Tap and double-click: `src-ts/pali_word.ts`

The word under a tap (mobile) or double-click (desktop) is what the word summary
looks up. The boundaries come from one regex, installed on `window` by
`simsapa.ts`:

```js
/[\p{L}\p{M}'’]+(?:[‘“"”]+[‘“"”'’]*n?ti[\p{L}\p{M}'’]*)?/gu
```

- A word is letters and combining marks plus `'` and `’`, which occur inside
  words (`manopubbaṅ'gamā`, `karissāmā’’ti`).
- It **continues across any run of quote marks into `ti` / `nti`**, and on into
  the word iti fused with, so `lābhanānattan”ti`, `dassanakāmo’”ti`,
  `sassatisaman”’ti`, `dhārayāmī'"ti` and `cā”tipi` are one word, and a tap on
  either half finds the whole word.
- It does **not** join `bhikkhave” tiṭṭhati` (a space) or `“Ehi, bhikkhu”`. It
  does join `bhante”ti` and `āvuso”tissa`, which are iti sandhi.

Users:

| entry point | file | how |
|---|---|---|
| mobile tap | `assets/js/suttas.js` | `window.ssp_word_at_offset(text, offset)`, then highlights that span |
| desktop double-click | `assets/js/suttas.js`, `assets/js/dictionary.js` | `window.ssp_expand_selection_to_word()` before the lookup |
| mobile long-press | `suttas.js` `selectionchange` | the browser's own selection, not widened |

**Why double-click needs widening.** The browser selects by Unicode word
boundaries (UAX #29). A single `’`, `‘` or `'` between two letters does not end a
word (MidNumLet / Single_Quote), but `”`, `“` and `"` always do, and so does a
run of two marks (`’’`). So a double-click on `lābhanānattan”ti` selects
`lābhanānattan` or `ti`.
`expand_selection_to_word()` widens the selection to the `pali_word.ts` span.
It only widens, and only within one text node.

**Trap: Windows also selects the trailing space.** A double-click on `ti` in
`bhante”ti so` selects `ti `. The widening compares without trailing
whitespace, or it would see a selection longer than the word and do nothing.

**Trap: `suttas.js` and `dictionary.js` run before `simsapa.min.js` loads.**
They may only call the `window.ssp_*` functions from event handlers, never at
script load. Each call is guarded by a `typeof … === "function"` check. The
bundle comes from the local server and can fail to load. Without the guard,
every tap and double-click lookup would then throw. With it, the tap falls back
to the plain letters-and-apostrophes word, and a double-click keeps the
browser's selection.

### The backend side of a tapped word

The tapped word reaches `/summary_query` with its quotes, and the backend's §3–4
handling resolves every quote variant to the same lookup. The JS only has to
deliver the whole word, never repair it.

### Find bar: `src-ts/find.ts`

`makeInterWordFlexible()` lets any run of whitespace, punctuation or quote marks
(`'’‘"“”`) stand between two find words, so a find query taken from
`content_plain`, such as the snippet-aware jump's, matches the rendered text.

An iti-sandhi boundary needs more: the query has `dhārayāmi ti` or `gantuṁ ti`,
the page has `dhārayāmī’ti`, `dhārayāmīti`, `gantun’ti`, `gantu’nti` or
`gantunti`. `markItiSandhi()` replaces each `<letter> ti` boundary in the term
with private-use placeholders, which accent folding and
`makeInterWordFlexible()` pass through, and `expandItiSandhiMarks()` turns them
into regex at the end:

- the letter before: `a`/`i`/`u` in either length, `ṁ` as `ṁ`, `ṃ`, `m` or `n`,
  with quote marks allowed before the `n` (`gantu’nti`);
- the gap before `ti`: zero or more quote marks or spaces.

It works with accent folding on or off. **Trap: the boundary must be marked
before folding.** Folding turns each letter into a class (`ti` into
`[tṭ][iī]`), so the boundary cannot be found reliably afterwards.

---

## 7. Decisions

1. **Split for search, rejoin for the dictionary.** Search compares stored text
   with the query, and both sides take the same split, so the split form is the
   right common ground there. DPD is keyed by the joined form, so its lookups
   rejoin. One shared normalizer plus one DPD-specific step keeps the two from
   drifting.
2. **The quote mark is the evidence of sandhi.** Quoted forms are always split;
   unquoted ones only where no ordinary ending looks the same (`mīti`, `unti`,
   `bhikkhūti`). An unquoted `magganti` stays a verb, while `magga’’nti` never
   becomes one.
3. **DPD's own analysis wins when it has one.** The joined key is used only when
   DPD breaks it down as `… + iti`; otherwise the app builds `<base> + iti`
   itself instead of trusting an unrelated key.
4. **No guessing on the sandhi paths.** No prefix guesses for the joined form or
   for the base word: a wrong analysis is worse than an honest `<base> + iti`
   with only *iti* found.
5. **One word rule on the page, matching the backend.** The JS regex extends a
   word into `ti` exactly when the backend would treat it as iti sandhi, so
   whatever is tapped can be resolved whole.
6. **Search splits at a smart quote inside a word; the dictionary and the
   Gloss tab join it** (§2). Search only needs the stored text and the query to
   agree, and a smart quote is usually punctuation. The dictionary and the gloss
   look up one word, so a compound's or an elision's apostrophe must not split
   it, and DPD has these words joined.
7. **Iti sandhi ends a word.** A `ti` with more letters after it is iti fused
   with the next word, which DPD has joined, so the split rules stop at a word
   end and the word stays whole.

---

## 8. Known gaps

- **A short base that DPD has wins over the long one.** `normalize_iti_sandhi`
  cannot tell a sandhi lengthening from a real long ending, so both the
  dictionary (§4) and the Gloss tab (§5) try the shortened base first and the
  long form only when the short one finds nothing.
- **Mobile long-press uses the browser's word selection**, which splits at `”`.
  The user can drag the selection handles over the whole word.
- **DPD coverage.** Many CST commentary words are in DPD in neither the joined
  nor the base form; they resolve to `<base> + iti` with only *iti* found.

---

## 9. Tests

| area | tests |
|---|---|
| iti split | `backend/src/helpers.rs`: `test_normalize_iti_sandhi_unquoted_rules_are_word_final`, `test_normalize_iti_sandhi_quoted_rules_are_word_final` |
| joining quote marks inside a word | `backend/src/helpers.rs`: `test_join_mid_word_apostrophes` |
| rejoin and DPD normalization | `backend/src/helpers.rs`: `test_normalize_dpd_query_text`, `test_normalize_dpd_query_text_apostrophes`, `test_rejoin_iti_sandhi` |
| DPD lookups | `backend/tests/test_dpd_lookup_grouped.rs`: `test_grouped_iti_sandhi_variants`, `test_grouped_iti_sandhi_not_a_verb_form`, `test_grouped_iti_sandhi_unknown_joined_form`, `test_grouped_iti_sandhi_long_vowel_base`, `test_grouped_not_iti_sandhi`, `test_dictionary_search_iti_sandhi`, `test_dpd_headword_prefix_columns_are_lowercase` |
| search queries vs stored text | `backend/tests/test_search_normalization.rs` (curly and straight quotes, `unti`, `contains_match_finds_dhovana_sutta`, `fulltext_match_finds_dhovana_sutta`) |
| possessive apostrophe | `backend/src/helpers.rs`: `test_split_possessive_apostrophe` |
| gloss extraction | `backend/src/helpers.rs`: `test_extract_words_keeps_apostrophe_compounds_whole`, `test_gloss_lookup_words`; `backend/tests/test_sandhi_split_detection.rs`, `test_extract_words_with_context.rs` (`test_sandhi_transformation_*`, `test_nti_sandhi_cases`); `test_gloss_word_resolution.rs`: `test_gloss_apostrophe_compounds_and_long_vowel_iti` |
| gloss context hash | `backend/src/helpers.rs`: `test_gloss_context_iti_sandhi_and_punctuation_invariance`; `backend/tests/test_gloss_word_resolution.rs`: `test_iti_sandhi_quote_variants_share_cache` |
| tap and double-click | `src-ts/pali_word.test.ts` |
| find bar | `src-ts/find.test.ts`, "Inter-word punctuation tolerance", "Iti-sandhi boundaries" |

---

## 10. Code map

| what | where |
|---|---|
| iti-sandhi regexes, `normalize_iti_sandhi`, `normalize_plain_text` | `backend/src/helpers.rs` |
| `remove_punct`, `compact_plain_text`, `split_possessive_apostrophe`, `normalize_fulltext_query`, `normalize_query_text` | `backend/src/helpers.rs` |
| `rejoin_iti_sandhi`, `join_mid_word_apostrophes`, `normalize_dpd_query_split`, `normalize_dpd_query_text` | `backend/src/helpers.rs` |
| `dpd_has_iti_breakdown`, `iti_base_candidates`, `dpd_lookup_grouped_iti_split`, `dpd_deconstructor_query`, `dpd_deconstructor_list`, `dpd_lookup`, `dpd_lookup_grouped` | `backend/src/db/dpd.rs` |
| per-mode query normalization, `dpd_query_text` | `backend/src/query_task.rs` `SearchQueryTask::new` |
| gloss extraction (`preprocess_text_for_word_extraction`, `extract_words_with_context`, `match_with_inner_apostrophes`, `is_quote_char`), `gloss_lookup_words`, `normalize_gloss_context` | `backend/src/helpers.rs`; `WordInfo.original_word` in `backend/src/types.rs` |
| word boundaries on the page | `src-ts/pali_word.ts`, installed in `src-ts/simsapa.ts` |
| tap, double-click, long-press | `assets/js/suttas.js`, `assets/js/dictionary.js` |
| find-bar punctuation and iti-sandhi tolerance | `src-ts/find.ts` `makeInterWordFlexible`, `markItiSandhi`, `expandItiSandhiMarks` |
