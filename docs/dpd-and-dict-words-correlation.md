# DPD records correlate to `dict_words` (structured data vs. rendered HTML)

`dpd_headwords` and `dpd_roots` records (in `dpd.sqlite3`) and `dict_words`
records (in `dictionaries.sqlite3`) are **two views of the same word**:

- The **structured** data (grammar fields, meanings, etc.) lives in the
  `dpd_headwords` / `dpd_roots` tables.
- The **rendered HTML** page for that same dpd_headword / dpd_root lives in its
  correlated `dict_words` record.

This is why `get_word_json` (returns structured rows from
`dpd_headwords`/`dpd_roots`/`dict_words`) and `render_word_html_by_uid` (resolves
via `dict_words`) appear to reach different record sets — they are the structured
and rendered-HTML views of the same word. To render a dpd_headword/dpd_root as
HTML, resolve to the correlated `dict_words` row (which holds the HTML); **there is
no separate DPD HTML renderer.**

**Why the uids differ across tables (bootstrap rationale).** During the CLI
bootstrap we import `dpd.sqlite3` (headword + root **structured** data, no HTML),
then separately import the **rendered HTML pages from a DPD StarDict export** to
build `dict_words`. The StarDict export is **keyed by `lemma_1`**, so
`dict_words.word` mirrors `dpd_headwords.lemma_1` one-to-one (a load-bearing
invariant — `SearchQueryTask::lemma_1_dpd_headword_match_ids` scans
`dict_words_word_fts.word` because of it). If `dict_words` used the `<row_id>/dpd` uid
format, **headword ids and root ids would collide** in the shared `dict_words`
table — so `dict_words.uid` is instead built from the sanitized lemma/root word
(`word_uid_sanitize(word) + "/dpd"`), which maps back **unambiguously** to the
right `dpd.sqlite3` row.

**The two uids are NOT the same string — correlation is `lemma_1` → sanitize, not
a uid string-equality join.** Real values from the shipped DB:

| `dpd_headwords` | | `dict_words` (HTML) |
|---|---|---|
| `id` | `uid` / `lemma_1` | `uid` / `word` |
| `34626` | `34626/dpd` / `dhamma 1.01` | `dhamma-1-01/dpd` / `dhamma 1.01` |

There is **no** `dict_words` row with uid `34626/dpd`. To reach the HTML row from a
numeric `<id>/dpd`: fetch the headword by id → `word_uid_sanitize(lemma_1)` +
`/dpd` → `get_word`. Consequently `34626/dpd` (→ dpd_headword structured row) and
`dhamma-1-01/dpd` (→ dict_words structured row) are **different structured records
of the same word**, both valid. Roots follow the same pattern but the disambiguated
form differs: `dpd_roots.uid` = `√akkh/dpd` (root key) while the `dict_words` row is
the sanitized root *word* (`√path-1/dpd` for `dpd_roots.root = "√path"` disambiguated
as `√path 1`).

> **Possible future bootstrap ergonomics improvement (not yet implemented):** add a
> nullable indexed `dpd_headword_id` / `dpd_root_id` column to `dict_words` (or a
> `dict_word_uid` column to the dpd tables), populated at bootstrap via the
> `lemma_1` join, so the cross-table mapping is a direct indexed lookup instead of
> a runtime `word_uid_sanitize` round-trip. Requires a re-bootstrap + DB version
> bump; the headword join is clean (`word = lemma_1`) but the root join needs care
> (disambiguation). See the API-tolerance task list for the trade-off analysis.

