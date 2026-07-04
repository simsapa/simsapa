# Text processing for ContainsMatch (FTS5) and FulltextMatch (Tantivy) Search

The sutta texts are in HTML format, which we convert to plain text for ContainsMatch (Sqlite FTS5) and FulltextMatch (Tantivy) search.

The plain text version is stored in `suttas.content_plain` field, converted from HTML with `helpers.rs::sutta_html_to_plain_text()`.

Processing flow:

- `sutta_html_to_plain_text()` -- removes header/footer boilerplate (see below), then:
- `compact_rich_text()` -- strips html
- `compact_plain_text()` -- nomalizes spaces
- `consistent_niggahita()` -- ensures ṁ
- `normalize_iti_sandhi()` -- `mūlan'ti` → `mūlaṁ ti`
- `remove_punct()`
  - all punctuation to spaces
  - normalize spaces, newlines, tabs
  - Remove remaining straight quotes: `'` and `"`, `manopubbaṅ'gamā` → `manopubbaṅgamā`

## Header / footer removal at bootstrap

The header/footer boilerplate is stripped **before** `compact_rich_text()` (i.e.
on the raw HTML, while the semantic tags are still present). Two content types,
three rules. Delivery is by **re-bootstrap** (regenerate `content_plain` /
`definition_plain` + rebuild the FTS5 / Tantivy indexes), not in-place migration.
All rules are **idempotent** (safe to re-run) and no-op when the structure is
absent. The `backend` `regex` crate is `regex = "1.0"` — it has **no
lookaround**, so "match up to the next block tag" is expressed by *enumerating the
allowed inner inline tags* (the match halts at the first block tag without
consuming it, so `replace_all` handles adjacent boilerplate correctly).

### Suttas — `sutta_html_to_plain_text()` (`backend/src/helpers.rs`)

Shared entry point for **all** sutta bootstrap sources (SuttaCentral HTML + Bilara
JSON, dhammatalks_org, nyanadipa, tipitaka_xml/CST, buddha_ujja,
dhammapada_munindo).

- **Header rule:** remove the whole `<header>…</header>` region **but preserve the
  inner `<h1>…</h1>`** — the sutta title, *including any leading reference number*
  (e.g. keep `239. Harita-Mata Jātaka`). Drops the nikāya / vagga / division /
  subdivision markup (SuttaCentral `<ul><li class='division'>…`, CST `<h3>` nikāya
  line). Multi-line safe (`(?s)` DOTALL — the previous non-DOTALL
  `<header(.*?)</header>` silently stripped *nothing* from multi-line headers). If
  a header has no `<h1>`, the whole header is removed. One HTML rule covers both
  sutta paths: the Bilara JSON path renders segments to HTML with an
  `<h1 class='sutta-title'>` title *before* the plain-text pass, so keying off the
  `:0.x` segment index is unnecessary (and unreliable — the title is `:0.3` in
  sn1.10 but `:0.4` in thag7.3).
- **Footer rule:** remove `<footer …>…</footer>` (multi-line safe). Both sutta
  paths run `bilara_html_post_process` first, which rewrites `<footer>` →
  `<footer class='noindex'>`, so the match is `<footer\b[^>]*>…</footer>` (covers
  bare + `noindex`). All `noindex` markers in the corpus are on `<footer>`, so this
  satisfies the `noindex` convention.

*Out of scope:* CST bodies embed nikāya/vagga heading names as **body** text (e.g.
`dn1.att/pli/cst` → `…dīghanikāye…`); the header rule does not touch body text.
Flagged as a possible follow-up, not part of this rule.

### DPD dictionary — `dpd_strip_footer()` (`backend/src/helpers.rs`)

Applied by the `strip_dpd_footers_from_plain()` DB pass (`backend/src/db/dpd.rs`,
wired into `cli/src/bootstrap/dpd.rs` before the dictionaries FTS5 indexes) which
recomputes only `definition_plain` (never `definition_html`) as
`compact_rich_text( dpd_strip_footer( dpd_strip_sutta_ref_paragraphs( definition_html ) ) )`.
Footer boilerplate is **interleaved with real content** (example verse, declension
/ conjugation table), so each structure is removed **in place — never
truncate-to-end**. The real example verse / grammar / declension divs and tables
are preserved. Four structures:

1. **Feedback prompts** — `<p class=dpd-footer>…` (≈3/entry, varied wording, all
   caught by the class). Unclosed `<p>`, nested `<a>`/`<br>`/`<span>`.
2. **Loading placeholders** — `<div …id=…>…loading...</div>` matched by **id
   prefix** (`family_word_` / `family_compound_` / `family_set_` / `frequency_` /
   `feedback_`), **not** by the `dpd content hidden` class (shared with the real
   `grammar_` / `example_` / `declension_` divs).
3. **Inflection-not-found note** — a bare unclosed `<p>Inflections not found in any
   Pāḷi corpus…`, matched by leading text (spans the nested `<span class=gray>`).
4. **Conjugation/declension-table feedback** — a bare unclosed `<p>Did you spot a
   mistake in the {conjugation|declension} table? … Report it here.</a>` inside the
   conjugation/declension div after its `</table>` (no class, no id), matched by
   leading text, halting at the closing `</div>`. *(Discovered during
   verification; the only source of the residual "report it here" leak — 442
   verb/declension entries.)*

`dpd_strip_sutta_ref_paragraphs()` was extended to also span the **converted**
`<p class="sutta"><a href="ssp://suttas/…">DISPLAY</a>` form (not just the bare
pre-conversion text), so the footer pass — which runs on post-conversion HTML —
does not leak the sutta display text back into `definition_plain`.

- ? When removing single and double quote marks, should we remove unicode smart quote?
Are there examples within compounds? `manopubbaṅ’gamā` (with smart quote)

`preprocess_text_for_word_extraction()`
- for glossing, dict matches
- `normalize_iti_sandhi()` -- `mūlan'ti` → `mūlaṁ ti`

- Should normalize to `word ti` or ``word nti`, separating the stem form
- stemmer will resolve `bhikkhūti` for fulltext
- contains match should match `bhikkhūti` when that is the query
- contains match should find `bhikkhūti` for the query `bhikkhu` or `bhikkhūti`
  - same for `bhikkhunti`, not the same problem as `-anti` verb endings

-----

Should match 'nti' variations and '-ati/-anti' verb endings.
Use fulltext stemmer to remove endings?

nibbānan”ti → nibbānaṁ ti

----

adhippeto bhikkhūti.

Stemmer should handle:
bhikkhūti → bhikkhu ti
bhikkhunti → bhikkhu ti

----

anabhijanam: no results

First page of results should include:

: pli-tv-bu-pm/pli/ms
: pli-tv-bu-vb-pj4/pli/ms
: Yo pana bhikkhu anabhijānaṁ uttarimanussadhammaṁ 

ContainsMatch 'anabhijānaṁ' - ok
Then switch to FulltextMatch - results ok, 'anabhijānaṁ uttarimanussadhammaṁ' shows
pli-tv-bu-vb-pj4/pli/ms but not pli-tv-bu-pm/pli/ms

'anabhijanam' FullText: no results
'anabhijanam uttarimanussadhammam' FullText: no results

----

anabhija
anabhijā -- ok results from both ms and cst, Pali stemmed from anabhijānaṁ

anabhijan
anabhijān -- no results, expected, not a valid stem for fulltext

anabhijana
anabhijāna -- results only from cst

vin01m.mul.xml/pli/cst includes:
: Yo pana bhikkhu anabhijānaṁ uttarimanussadhammaṁ
What matches from fulltext in search results:

Anabhijānanti asantaṁ → Anabhijāna nti asantaṁ

anabhijana + m: no results
anabhijāna + ṁ: results from ms and cst

-----

Typing anabhijānaṁ: first showing results with ms and cst suttas, but another query was launched and returns with cst only results

And old query overrides the results from a new query

-----

query:
nandi dukkhassa mula
nandi dukkhassa mulam

Should show
mn1/pli/ms
: ‘Nandī dukkhassa mūlan’ti
mn1/pli/cst
: ‘Nandī [nandi (sī. syā.)] dukkhassa mūla’nti
: ‘Nandī dukkhassa mūla’nti

mn1/pli/ms: nandi dukkhassa mulan ti iti -- doesn't match, 'mulan'

mn1/pli/cst: nandi dukkhassa mula nti iti -- matches


