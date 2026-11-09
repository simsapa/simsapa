# DPD analysis reports: source of these files

These are outputs of the Digital Pāḷi Dictionary passage analysis tool
(`exporter/analysis/` in the `dpd-db` repository), copied unchanged. They are
the reference for the Word in Sentence conformance test
(`backend/tests/test_dpd_analysis_conformance.rs`). Never edit them to make a
test pass: a row that disagrees because Simsapa ships a different DPD release
is reported by the test, not repaired here.

Copied on 2026-11-09.

## `fixtures/` — DPD's regression fixtures

From `dpd-db` at commit `36b75c7972475fa80fa764918fda9ef8a0cf9b7c`
(2026-11-08), `tests/exporter/analysis/fixtures/passages/<name>/`:

| Fixture | DPD passage code | Paragraph |
|---|---|---|
| `TH215` | TH215 | 1 |
| `MN41_p2` | MN41 | 2 |
| `SN15.1_p2` | SN15.1 | 2 |
| `DHP211` | DHP211 | 1 |
| `AN3.33_p1` | AN3.33 | 1 |

Files per fixture:

- `study.md` — the rendered report.
- `metadata.json` — the stdin given to DPD's `study_passage.py` (passage code
  and paragraph number) and the AI provider and model that made the
  selections. It does **not** record the DPD data version.
- `distilled.json` — DPD's own regression summary: one entry per top-level
  token, `[word, occurrence, chosen option key, grammar, meaning]`, made by
  `extract_distilled_rows()` in `tests/exporter/analysis/test_passage_regression.py`.
  - `occurrence` is the 1-based count of that surface word so far in the
    passage (the second `na` in DHP211 is `2`).
  - The option key is `w<token index>_<rest>`, where `<rest>` is
    `<headword id>_<i>` (the `i`-th triple of the token's `lookup.grammar`),
    `<headword id>_default` (no triple matched; the grammar shown is the
    headword's own `grammar` field), or `decon_<lookup key>_<n>` (the `n`-th
    deconstructor split of the lookup row; no headword id).
  - `grammar` is the option's grammar *before* rendering, so it lacks the
    `adj, ` prefix that `study.md` shows for adjectives.

**`distilled.json` and `study.md` disagree on six tokens**, so they come from
different runs. In each the word and the grammar text are the same and the
chosen headword differs:

| Fixture | Token | `distilled.json` | `study.md` |
|---|---|---|---|
| `MN41_p2` | 84 `lokaṃ` | 55973 `loka 2` | 55972 `loka 1` |
| `SN15.1_p2` | 21 `chetvā` | 27734 | 27732 |
| `SN15.1_p2` | 30 `nikkhipeyya` | 36804 | 36803 |
| `SN15.1_p2` | 37 `ayaṃ` | 8709, `masc acc sg of aya` | 13779, `fem nom sg of ima` |
| `SN15.1_p2` | 55 `hetu` | 89413, `ind, adv` | 71433, `masc nom sg of hetu` |
| `AN3.33_p1` | 15 `āyasmantaṃ` | 12242 | 12241 |

They are kept as copied. The conformance test therefore replays each fixture's
choices from its `study.md` (ID and Grammar text, as for the examples), and
`test_dpd_report_parser.rs` asserts that exactly these six differ.

Not copied: `study.json` (the full candidate analysis, 0.15–1.5 MB each) and
`ai_debug.json` (the recorded AI exchanges, up to 1.9 MB each).

The DPD data version behind these fixtures is not recorded. Simsapa's
`dpd.sqlite3` at the time of copying is DPD `v0.4.20261026`
(`db_info.dpd_release_version`).

## `examples/` — example reports

From the `dpd-analysis-examples` repository at commit
`93faafb3cc15cb8ec1aaa7b9b5ffd82e489d1f3c` (2026-11-08): `DHP23`, `ITI1`,
`SN18.1_p2`, `UD1`, `UD12` (`<code>_study.md`). These have the report only; the
conformance test replays each row's ID and Grammar text.

## Text conventions

DPD writes the niggahīta as `ṃ` and inserts apostrophes from its speech-mark
table (`vā'ti`, `pan'āniccaṃ`, `eso'ham'asmi`). Simsapa writes `ṁ`; the test
normalises `ṃ` → `ṁ` and treats straight and curly apostrophes as equal.
