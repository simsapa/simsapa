# DPD `construction_summary`: a bootstrap column taken from DPD's rendered entry

`dpd_headwords.construction_summary` holds the construction **as DPD displays
it**: `√car + a + ti` for *carati 1.1*, whose stored `construction` column is
`cara + ti`. `POST /lookup_forms`, exact `/gloss_text` and `/word.json` return
it (see [the API doc §19.1](./simsapa-localhost-api-search-endpoints.md#191-tier-1-post-lookup_forms)).
Nothing in the GUI reads it.

## 1. Where the value comes from

Every rendered DPD headword page ends its summary line with the construction
in brackets, made by DPD's own display rule (dpd-db's
`tools/meaning_construction.py`, `summarize_construction()`), which replaces
the root base with root + sign:

| headword | stored `construction` | rendered summary line |
|---|---|---|
| `bhaṇāmase` | `bhaṇa + mase` | `imp. (+acc) <b>we must speak!; let us say!</b> [√bhaṇ + a + mase] <span class=gray>✔</span>` |
| `carati 1.1` | `cara + ti` | `pr. (+acc) <b>walks; wanders; …</b> [√car + a + ti] <span class=gray>✔</span>` |
| `bhagavant 1` | `√bhaj > bhag + a + vant` / `bhaga + vant` | `masc. <b>Sublime One; …</b>; lit. having good fortune quality [√bhaj + a + vant] <span class=gray>✔</span>` |

The value is cut out of that line, **not** computed with a port of DPD's
Python rule. The page is DPD's own output of the rule, for the same DPD
release, so the value is exactly what DPD shows its readers, including any
change to the rule in a later release. A port would need re-checking against
dpd-db on every DPD update.

The pages are already in `dictionaries.sqlite3` (`dict_words`,
`dict_label = 'dpd'`), imported from the DPD StarDict export by
`dpd_bootstrap()` before `dpd_migrate()` runs. `dict_words.word` equals
`dpd_headwords.lemma_1` one to one, so each headword's page is found by
`word = lemma_1`. The StarDict export and `dpd.db` both come from
`dpd-db-for-bootstrap/current/`; if they were ever taken from different DPD
releases, the "meaning and construction but no summary" count in §3 would
show it.

Some headwords without a `meaning_1` still show a bracket (`akaṭa 3`
`[√kar]`, `bhagavant 2` `[√bhaj]`): DPD's root-family fallback. Those are
taken as rendered.

## 2. The line shape the extractor relies on

`extract_dpd_construction_summary()` (`backend/src/dpd_form_grammar.rs`, pure,
unit-tested) returns `Found(text)`, `NoBracket` or `Malformed`:

- The summary line runs from the first `<div class=dpd><p>` to the next
  `</div>`. No such div → `NoBracket`.
- No `[` in the line → `NoBracket` (written as `""`).
- Exactly one `[`, followed later by `] <span class=gray>`, with no `<`, `&`
  or `]` between them → `Found`, trimmed.
- Anything else → `Malformed`, written as `""` and reported. The extractor
  never guesses.

Measured on `v0.4.20260501` (88,864 headwords): every headword has its page;
75,595 summary lines have a bracket, each with exactly one `[` immediately
followed by `] <span class=gray>` at its end, and no tag, entity, ` > ` or `(`
inside; every headword with both a `meaning_1` and a `construction` has one.
The 13,269 without a bracket all lack a `construction`, a `meaning_1`, or both.

The bootstrap's earlier rewrites of `definition_html`
(`convert_dpd_example_sutta_links`, `convert_dpd_epd_word_links`) do not touch
the summary line; the malformed count would show it if one ever did.

## 3. The bootstrap step

`populate_dpd_construction_summaries(dpd_db_path)` in `backend/src/db/dpd.rs`:

1. adds `construction_summary VARCHAR NOT NULL DEFAULT ''` to `dpd_headwords`
   if `PRAGMA table_info` lacks it, so it can run on an existing DB;
2. reads `(word, summary_line)` for the DPD pages through
   `get_app_data().dbm.dictionaries`, **cutting the line out in SQL** with
   `substr` / `instr`. `dict_words` holds about 840 MB of DPD
   `definition_html` and Diesel's `sql_query` loads the whole result, so the
   HTML itself is never fetched (the line averages 170 bytes);
3. writes a value to **every** headword in one transaction (`""` for no page,
   no bracket or a malformed line), so a re-run gives a complete column.

`import_migrate_dpd()` calls it after the DB is moved into place and before
`create_dpd_indexes()`, so both the full bootstrap and the
`import-migrate-dpd` command fill the column. The column has no index (it is
read by primary key), and the bootstrap's `ANALYZE` of `dpd.sqlite3` needs
nothing more.

**The report** (`ConstructionSummaryReport::log()`) is one `info` line with
headwords updated, with a summary, without, and malformed lines, then two
`warn` lists: the malformed lines (`lemma_1` + the line) and the headwords that
have both `meaning_1` and `construction` but got no summary. Each list logs its
total and its first 20 items only, so a DPD template change or a `--limit`
bootstrap (which imports only part of the StarDict) cannot write tens of
thousands of lines. The report never fails the bootstrap: source data is
reported, never repaired. Expected on `v0.4.20260501`:

```
construction_summary: 88864 headwords updated, 75595 with a summary, 13269 without, 0 malformed summary lines
```

with neither warning list. A non-zero malformed count means DPD changed its
template: read the logged lines before loosening the extractor.

**On an existing `dpd.sqlite3`**, without a full bootstrap:

```sh
cd bootstrap_cli && SIMSAPA_DIR=<the simsapa dir whose dictionaries.sqlite3 holds the DPD pages> \
  cargo run --release -- populate-dpd-construction-summary <path>/dpd.sqlite3
```

It writes the DB in place (`ALTER TABLE` + an `UPDATE` of every row), so run
it with the app closed, then restart the app (§4). `SIMSAPA_DIR` decides which
`dictionaries.sqlite3` the pages are read from; it must be from the same DPD
release as the `dpd.sqlite3`. The result has the same column and values as a
full bootstrap, but not the same file layout, and it does not rebuild the
`dpd.tar.bz2` release archive.

## 4. Never in the Diesel model

**Do not add `construction_summary` to `dpd_schema.rs` or to `DpdHeadword`.**
Diesel's `table.load::<DpdHeadword>()` selects every column in the `table!`
macro, so adding it there would make **every** headword query fail against an
installed `dpd.sqlite3` without the column. Installed copies are kept until a
DB version bump makes the app download a new one, so the app must work with
and without it.

It is written with `sql_query` at bootstrap, and read at runtime with
`construction_summaries_by_ids()` (`backend/src/db/dpd.rs`), one chunked
`SELECT id, construction_summary … WHERE id IN (…)` per request:

- Column presence is checked once per process with
  `PRAGMA table_info(dpd_headwords)` (`has_construction_summary_column()`) and
  cached in a `OnceLock<bool>`. When it is absent the function returns
  `Ok(None)` without querying and logs one `warn`, and the API fields are
  `null`. Presence is never detected by matching SQLite's "no such column"
  error text.
- Because of the cache, a DB that gains the column while the app runs is seen
  only after a restart.
- Any other DB error fails the request (`500`).

`/word.json` adds the field through `AppData::word_json_with_extras()`, never
inside `AppData::resolve_word_uid()`: the resolver is also called by the word
page renderer (GUI and `/word_html`), and the extra query would run on every
word page.

`dpd.sqlite3` has no migrations (see
[database-migrations.md](./database-migrations.md)); this column is the
bootstrap's job, and the app tolerates its absence instead of migrating it.
