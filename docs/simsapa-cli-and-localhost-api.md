# The `simsapa` CLI and the Simsapa localhost API

Simsapa's sutta search, dictionaries and Pāli glossing can be used without the
GUI, in two ways that answer from the same code:

- **The `simsapa` command-line program** (npm package `@simsapa/cli`): one
  command per question, readable text by default, or with `--json` the same
  JSON document as the matching API route. Part A.
- **The localhost HTTP API**: a local server (Rocket) for the browser
  extension, agents and other local clients, provided by the GUI app while it
  runs or by `simsapa serve`. Part B, §0–§19.

This document is compiled into the `simsapa` binary, so the reference always
matches the installed version:

```sh
simsapa docs                                   # print it (Markdown)
simsapa docs --output simsapa-reference.md     # save it to a file
```

It is self-contained: everything a client needs is here.

---

# Part A. The `simsapa` command-line program

## A.1 Install and set up

The `simsapa` program is on npm as `@simsapa/cli` (Node.js 18 or later; Linux
x64 and arm64, macOS on Apple Silicon, Windows x64). The package carries a
native binary and downloads nothing at install time; `simsapa setup` then
downloads the databases (about 4.8 GB on disk once extracted):

```sh
npm install -g @simsapa/cli
simsapa setup              # asks before downloading; --yes for scripts
simsapa search suttas "four noble truths"
```

`setup` uses the GUI app's own data folder (on Linux `~/.local/share/simsapa`),
so on a machine where the app is already set up it reports so and downloads
nothing; `--simsapa-dir <path>` (or `SIMSAPA_DIR`) chooses another folder for
any command. `simsapa setup --languages de,hu` adds sutta languages,
`--list-languages` lists them. `simsapa paths` shows the folder and which
databases exist; any other command run before `setup` says so and exits 2.

## A.2 Commands

`simsapa --help` lists them and `simsapa <command> --help` gives every option.
The read-only ones, each with the route whose JSON its `--json` prints:

| Command | What it answers | `--json` is the body of |
|---|---|---|
| `simsapa search suttas\|dict\|library <query…>` | Search, with `--mode` (the API names of §3: `"Fulltext Match"`, `"Contains Match"`, `"Title Match"`, `"Headword Match"`, `"DPD Lookup"`, …), `--lang`, `--source`, `--page`, `--page-len`, `--all-snippets`, `--snippet-exclude`. Defaults: `Fulltext Match` for suttas and library, `Combined` for dict | `POST /search` (§7) |
| `simsapa word <uid>` | A dictionary entry; `--html` for the rendered page | `/word.json` (§13.3); `--html`: `/word_html` |
| `simsapa sutta <uid>` | A sutta's title, uid and plain text; `--html`, `--content-json` (Bilara JSON), `--bilara` (with variants, comments, glosses) | `/sutta_html`, `/sutta.json` (§13.4), `/sutta_bilara.json` (§13.5) |
| `simsapa gloss <text…\|->` | Every word of a passage with its dictionary options and compound break-downs; `--exact` for one exact DPD lookup per word | `POST /gloss_text` (§16.1, §19.2) |
| `simsapa lookup <form…\|->` | Exact DPD form lookup, forms as arguments or one per line on stdin | `POST /lookup_forms` (§19.1) |
| `simsapa query suttas\|words <text>` | Literal substring match, first page; `--titles`, `--count` | — |
| `simsapa fulltext-search …` | The fulltext index directly | — |
| `simsapa health` | Versions (app, DPD, SuttaCentral data), database paths, counts, fulltext index status | `GET /health` (§10) |
| `simsapa stats`, `simsapa paths` | Row counts of `appdata.sqlite3`; the data folder and database paths | — |
| `simsapa docs` | This document | — |

The commands that change the data folder: `simsapa setup`, `simsapa import
stardict|stardict-zip|epub|html …` (a user dictionary or book, searchable in the
GUI app too), `simsapa index build|rebuild|delete …`, and `simsapa serve`
(§10.1).

Uids are the ones search results carry (`sn56.11/pli/ms`, `34626/dpd`,
`dhamma-1-01/dpd`); `sutta` also takes a reference (`mn1`, `SN 56.11`,
`dhp182`) and `word` a display form (`"dhamma 1.01"`), as the routes do (§13).

## A.3 Output: clean text, or `--json`

The default output is compact plain text meant to be read, by a person or an
agent: HTML stripped, one result per block, the matched terms in **bold** on a
terminal and as `**…**` when stdout is piped. Progress, spinners and log lines
go to stderr, so stdout holds only the answer.

```sh
$ simsapa search suttas vedanā aniccā --lang pli --page-len 2
2616 results (page 1/1308)

Vedanāsutta
  sn18.5/pli/ms
  ghānasamphassajā **vedanā** jivhāsamphassajā **vedanā** kāyasamphassajā **vedanā** manosamphassajā **vedanā** niccā vā **aniccā** va ti **aniccā** bhante evaṁ passaṁ rāhula sutavā ariyasāvako

5. Vedanāsuttaṁ
  sn18.5/pli/cst
  sotasamphassajā **vedanā** pe ghānasamphassajā **vedanā** jivhāsamphassajā **vedanā** kāyasamphassajā **vedanā** manosamphassajā **vedanā** niccā vā **aniccā** va ti **aniccā** bhante pe evaṁ passaṁ rāhula

$ simsapa search dict dhamma --page-len 1
5662 results (page 1/5662)

dhamma 1.01
  34626/dpd
  (masc) nature; character [√dhar + ma] masc, from dharati

$ simsapa word 34626/dpd
34626/dpd
Lemma: dhamma 1.01
Part of speech: masc
Grammar: masc, from dharati
Meaning: nature; character
Root: √dhar 1
Construction: √dhar + ma
…

$ simsapa gloss "atthaññe dhammā"
atthaññe  (pr) there is; there exists [√as + ti > atthi] pr  [atthi + aññe]
dhammā    (fem) name of an arahant nun [√dhar + ma + ā], [dhamma + ā] fem, from dhamma

$ simsapa lookup bhagavantaṁ bhagav
bhagavantaṁ  bhagavantaṁ  found  bhagavant 1, bhagavant 2
bhagav  bhagav  not_found
```

The text output shows the first, most useful part of each answer (for `gloss`,
each word's first dictionary option). **When an agent needs every field, it
should pass `--json` instead** and read the JSON, which is exactly the
matching route's response body as documented in Part B. An agent may also
write its own small formatter (for example a Python script that reads the
JSON on stdin) when it wants a shape the text output does not give. Under
`--json`, stdout holds exactly one JSON document; an error is
`{"error": "<message>"}` on stdout with a non-zero exit code.

```sh
simsapa search suttas dukkha --json | jq -r '.results[].uid'
simsapa gloss --json - < passage.txt > gloss.json
```

### Fulltext query syntax

`Fulltext Match`, the default sutta search, uses
[tantivy's query syntax](https://docs.rs/tantivy/latest/tantivy/query/struct.QueryParser.html).
It matches Pāli declensions, and words need not be next to each other: **so ce
evam vadeyya** also finds **so ce** bhikkhu **evaṁ vadeyya**. A `+` or `-`
prefix makes a term required or excluded:

- **santam padam abhisamecca**: each term may be included, but it is fine if
  not all are found.
- **santam padam +abhisamecca**: `abhisamecca` must be included.
- **santam padam -abhisamecca**: `abhisamecca` must not be included.

It does not match partial words: **upasan** finds nothing (not a valid
declension stem) until the word is complete, **upasankama**. For an exact
partial match use `--mode "Contains Match"`.

## A.4 Exit codes

| Code | Meaning |
|---|---|
| 0 | success |
| 1 | error (including a command-line usage error) |
| 2 | the databases are not set up (run `simsapa setup`) |
| 3 | the data folder is in use by the Simsapa app or another `simsapa` command |
| 4 | not found (an unknown uid or reference; a `lookup` form that is `not_found` is a result, not exit 4) |
| 130 | interrupted |

## A.5 Alongside the GUI app

The CLI and the GUI app share the data folder, the databases and the settings.
While the app is running, the read-only commands work as usual, but `serve`,
`setup`, `import` and `index` refuse with exit code 3 and name the app's own
server to use instead (§10.1). Close the app first to change the folder.

## A.6 One command per question, or `simsapa serve`

Each command opens the databases afresh: 130–250 ms per one-shot search with
the release binaries. That is fine for occasional questions. An agent making
many queries should start `simsapa serve` once and call the API (Part B)
instead; the server answers from already open databases.

## A.7 Running the commands without a permission prompt each time

The read-only commands of A.2 (`search`, `word`, `sutta`, `gloss`, `lookup`,
`query`, `health`, `paths`, `docs` without `--output`) only read the user's
data, so an agent that is to search autonomously can ask the user to add
`simsapa` with those subcommands to its tool-permission allowlist, plus `jq`
or `grep` to filter the output. With `simsapa serve` running, `curl` to the
`127.0.0.1` server is read-only in the same way, except for the two writing
routes named in §10.1. Allow-listing these lets the agent run the whole
search → copy `uid` → fetch-full-text loop without a prompt per call, while
everything else stays gated.

---

# Part B. The localhost API

The default port is 4848. Two programs provide the server:

- **The GUI app**, bound to `127.0.0.1:<api_port>` while it runs. It serves
  every route in this document.
- **`simsapa serve`**, for machines without the GUI (bound to `127.0.0.1`
  unless `--host` says otherwise, §10.1). It serves the **shared** data routes
  only: search, word and sutta retrieval, `/gloss_text`, `/lookup_forms`,
  `/health` and the rest of §14.1 except the AI routes. The **GUI-only** routes
  (§14.2, which drive the app's windows, plus the clipboard, external-URL and
  `/app-assets-list` routes of §14.3), the **AI routes** (`/word_selection_ws`,
  `/set_ai_provider_key`, `/ai_provider_key`) and the **static-asset routes**
  (`/assets/…`, `/favicon.ico`, `/get_pdf_viewer/…`, `/book_resources/…`,
  `/dict_resources/…`) are a plain `404` there.

Paths, JSON and status codes of the shared routes are the same from both. HTML pages rendered by `simsapa serve` still
link `{api_url}/assets/…` (fonts, scripts, icons), which 404 there: the reader
CSS is inlined in the page, so the text an agent extracts is unaffected.

```sh
simsapa serve              # Simsapa API listening on http://127.0.0.1:4848
curl -s -X POST "localhost:4848/suttas_fulltext_search" \
  -H 'Content-Type: application/json' \
  -d '{"query_text":"vedanā aniccā","suttas_lang":"pli","page_num":0,"page_len":20}' \
  | jq -r '.results[] | "\(.uid)  \(.title)"'
```

Part B covers the **whole route surface**. The four **search** endpoints
and the **sutta/dictionary retrieval** routes are documented in detail (request /
response JSON, parameter structs, curl examples) in §1–§13; **every other route**
is catalogued with its purpose in the complete route reference (§14), with the
remaining request/response structs in §15. A sutta's text as **SuttaCentral
(Bilara) JSON**, segment id → text, is `GET /sutta.json?uid=` (§13.4); the same
text with its **variant readings, comments, glosses and HTML template** is
`GET /sutta_bilara.json?uid=` (§13.5). The **gloss pipeline** routes
(`POST /gloss_text` and the `GET /word_selection_ws` WebSocket) — for external
clients building a two-step "Glossing… → Word Selection…" table UI — are in §16.
**Reference conversion** (`POST /reference_search`: a PTS volume and page to its
SuttaCentral reference, and back) is in §17.
**Iti sandhi** (`vuttan'ti`, `satipaṭṭhānāti`, `vuttan ti`): which route resolves
it and what each returns is §18. **How exact a dictionary answer is**, and how
to widen a lookup step by step (`/lookup_forms` → `/dict_combined_search` →
`/search` Headword / Contains Match), is §19. The **exact DPD form lookup**,
`POST /lookup_forms` (up to 5,000 forms per call, one exact, unranked result per
form, in input order), and `/gloss_text`'s **`"exact": true` mode** (one such
result per word of a passage, nothing dropped or chosen) are §19.1–§19.2. Each
result carries DPD's own parse of the form (`form_grammar_raw`, and per
headword `form_grammar`) and each headword the construction as DPD displays
it (`construction_summary`).
`GET /health` reports `dpd_version`, the cache key for DPD-derived answers,
and `sc_data_version`, the SuttaCentral data the suttas were imported from
(§10).

All search routes run the app's own search, so the returned results carry
exactly the same **non-nested**
`<span class='match'>` highlighting, `is_snippet` markers, per-occurrence
snippet expansion, and snippet-exclusion behaviour as the in-app results.

---

## 0. Quick start (agents): search → copy `uid` → fetch the full entry

The happy path is two calls: **search** to find a row, then **retrieve** its full
text by the `uid` from the result. Snippets come from search; full text comes from
the retrieval routes (§13). Read the live port from `<SIMSAPA_DIR>/api-port.txt`
(default `4848`).

```sh
PORT=$(cat "$SIMSAPA_DIR/api-port.txt" 2>/dev/null || echo 4848)

# (optional) one-shot environment probe: version, DB paths, counts, installed dicts
curl -s "localhost:$PORT/health" | jq '{app_version, counts, dict_sources}'

# --- Suttas -------------------------------------------------------------------
# 1. Search (fulltext). Copy the .results[].uid you want (e.g. "sn56.11/pli/ms").
curl -s -X POST "localhost:$PORT/suttas_fulltext_search" \
  -H 'Content-Type: application/json' -d '{"query_text":"dukkha"}' | jq '.results[].uid'
# 2. Retrieve the full rendered HTML for that uid (then strip tags for plain text):
curl -s "localhost:$PORT/get_sutta_html_by_uid/web/sn56.11/pli/ms"
# 2b. …or the segmented text as SuttaCentral (Bilara) JSON, segment id → text (§13.4):
curl -s -G "localhost:$PORT/sutta.json" --data-urlencode "uid=sn56.11/pli/ms" | jq .
# 2c. …or that text together with its variant readings, comments and glosses (§13.5):
curl -s -G "localhost:$PORT/sutta_bilara.json" --data-urlencode "uid=sn56.11/pli/ms" | jq '.variants'

# --- Dictionary ---------------------------------------------------------------
# 1. Search DPD. Copy a .results[].uid (e.g. "dhamma-1-01/dpd" or numeric "34626/dpd").
curl -s -X POST "localhost:$PORT/dict_combined_search" \
  -H 'Content-Type: application/json' -d '{"query_text":"dhamma"}' | jq '.results[].uid'
# 2a. Structured record as JSON (for glossary export / grammar fields):
curl -s "localhost:$PORT/words/dhamma-1-01/dpd.json" | jq '.[0]'
# 2b. …or the rendered entry HTML:
curl -s "localhost:$PORT/get_word_html_by_uid/web/dhamma-1-01/dpd"
# Checking whether DPD knows forms? Use /lookup_forms, not /dict_combined_search:
# exact, batched, and its "not_found" can be trusted (§19.1).
curl -s -X POST "localhost:$PORT/lookup_forms" \
  -H 'Content-Type: application/json' -d '{"forms":["bhagavantaṁ","bhagav"]}' | jq '.results[].status'
```

Three things that remove guesswork (all detailed below):

- **Always copy `uid` from a search result** rather than hand-building it. The
  retrieval routes are tolerant (human display forms like `dhamma 1.01`, numeric
  `34626/dpd`, and hyphenated `dhamma-1-01/dpd` all resolve — §13.3), but the
  result `uid` is guaranteed to work.
- **If you must percent-encode the `uid`’s `/`, use the query-param routes**
  (`/word.json?uid=`, `/word_html?…`, `/sutta_html?…`) — the `<uid..>` *path*
  routes reject `%2F` with HTTP 422 (§13, §13.3).
- **A miss is HTTP 404** (the JSON route still returns a `[]` body); add
  `?verbose=1` to `word.json` for a `{found, canonical_uid, hint}` envelope (§13.3).
- **Start exact, then widen** (§19): `/lookup_forms` answers exactly and its
  `not_found` can be trusted; `/dict_combined_search` adds the app's respellings
  and guesses; `/search` with `Headword Match` / `Contains Match` casts the
  widest net. Send an iti-sandhi word whole, `ti` included (§18).

Without a server, each of these calls has a `simsapa` command printing the same
JSON with `--json` (A.2): `simsapa search suttas dukkha --json`,
`simsapa sutta sn56.11/pli/ms --content-json`, `simsapa word dhamma-1-01/dpd --json`,
`simsapa lookup bhagavantaṁ bhagav --json`.

---

## 1. The four endpoints

| Route | Default mode (non-reference) | Area | Deconstructor |
|-------|------------------------------|------|---------------|
| `POST /search` | area-specific (see §4) | any (request-driven) | Dictionary only |
| `POST /suttas_fulltext_search` | `FulltextMatch` (tantivy) | Suttas | `None` |
| `POST /suttas_contains_search` | `ContainsMatch` (literal) | Suttas | `None` |
| `POST /dict_combined_search` | `DpdLookup` | Dictionary | yes |

These are **search** routes: they rank, respell and guess. To check whether DPD
knows a form *exactly*, use `POST /lookup_forms` instead (§19.1).

> **Note — `/suttas_fulltext_search` changed.** It previously ran `ContainsMatch`
> (despite the name). It now runs real `FulltextMatch` (tantivy). No legacy alias
> is kept; clients wanting literal substring matching use
> `/suttas_contains_search`.

> **Caveat — the Tantivy fulltext index is Pāli-stemmed AND
> diacritic-insensitive.** `FulltextMatch` is best for *discovering* a lemma
> across all its inflected case forms (best recall), but because the stemmer
> folds diacritics, `nāvā` ("boat") also matches `nava` ("nine"). Treat fulltext
> hits as candidates and **verify by retrieving the actual text** (see §13). When
> you already know the exact wording, prefer `/suttas_contains_search` (literal
> substring) for precision.

---

## 2. Request shape (`ApiSearchRequest`)

All fields except `query_text` are optional; serde deserializes a missing
`Option` field to `None`, so existing clients that send only a subset keep
working unchanged.

```jsonc
{
  "query_text": "pajahati",        // required
  "page_num": 0,                    // default 0
  "page_len": 20,                   // default 20

  // General /search only (the named routes hardcode mode + area):
  "mode": "Fulltext Match",         // exact SearchMode serde label (see §3)
  "search_area": "Suttas",          // exact SearchArea serde label (see §3)

  // Suttas/Library areas:
  "suttas_lang": "en",
  "suttas_lang_include": true,
  "show_all_snippets": true,        // default false; per-occurrence expansion
  "snippet_exclude": ["upādiyati"], // JSON array (NOT a CSV string)

  // Dictionary area:
  "dict_lang": "en",
  "dict_lang_include": true,
  "dict_dict": "PTS",
  "dict_dict_include": true
}
```

- `snippet_exclude` is an **already-split array**; the API does not split a
  comma-separated string.
- The language/source filters treat the placeholder values `"Languages"` /
  `"Language"` (and `"Dictionaries"` / `"Dictionary"` for the source) — and the
  empty string — as **no filter**.

## 3. Exact mode / area serde names

Request strings must match the `SearchMode` / `SearchArea` labels exactly; an
unrecognized value on `/search` returns **HTTP 400**.

`mode`:
`"Combined"`, `"Fulltext Match"`, `"Contains Match"`, `"Headword Match"`,
`"Title Match"`, `"DPD ID Match"`, `"DPD Lookup"`, `"Uid Match"`,
`"RegEx Match"`.

`search_area`: `"Suttas"`, `"Library"`, `"Dictionary"`.

## 4. `POST /search` mode / area resolution

- `search_area` defaults to `"Suttas"` when omitted; an unknown value → 400.
- `mode` defaults are **area-specific** (the first entry of the app's search
  mode dropdown): Suttas/Library → `"Fulltext Match"`, Dictionary →
  `"Combined"`. An explicitly-sent unknown `mode` → 400.
- `/search` honors the requested mode **strictly** — there is no
  reference → `UidMatch` override (that lives only on the named convenience
  routes, see §5).

**Dictionary `Combined` is special.** `SearchQueryTask` rejects
`Combined + Dictionary` (it is bridge-orchestrated and would error → empty
results). So when `/search` resolves to `Dictionary` + `Combined` (the default,
or an explicit request), it maps it to the **`/dict_combined_search` behaviour**:
a UID-pattern query → `UidMatch`, otherwise `DpdLookup`, plus the `deconstructor`.
For Suttas/Library, `Combined` is fine — `results_page` maps it to
`FulltextMatch` internally.

For the Dictionary area, `/search` applies the dict language/source filters and
returns the `deconstructor` (computed from the original query via
`dpd_deconstructor_list`), so `/search` is a strict superset of
`/dict_combined_search`. For Suttas/Library it applies the suttas language
filter and `deconstructor` is `None`.

## 5. Named-route UID auto-detect (self-correcting)

`/suttas_fulltext_search` and `/suttas_contains_search` keep a sutta-reference
auto-detect: if the query is a sutta reference (e.g. `"sn56.11"`, `"MN 44"`,
`"dhp182"`), the route runs `UidMatch` instead of its fallback mode.
`/dict_combined_search` does the same for dictionary uids, and tries **only**
the dictionary uid patterns — so `"ja 1.1"` is the DPD headword there and the Jātaka in the
sutta routes. The area is what keeps the two from colliding; there is no
cross-area exclusion list.

Only a query that is **entirely** a reference is rewritten (`SN 56:11`,
`Dhp 1-20` and `DN 1.` all are). A reference inside a longer query stays text:
the result is searched in the route's own mode, where an embedded `uid:mn118`
is literal text to ContainsMatch and matches nothing in FulltextMatch (the
Tantivy `uid` field is raw and holds the full `mn118/pli/ms`). `/search` does **not** do this (mode is
strict — see §4).

**No silent 0-hit — the auto `UidMatch` self-corrects.** When the
auto-detected `UidMatch` finds **nothing** (the human form differs from the
stored uid — e.g. the display title `dhamma 1.01` is stored as
`dhamma-1-01/dpd`), the route transparently re-runs before returning, so a
uid-like query that *looks* right but doesn't match a stored uid no longer comes
back empty:

- **`/dict_combined_search`** (and `/search`'s Dictionary `Combined` path) falls
  back in order to (1) `UidMatch` on the **normalized** uid
  (`dhamma 1.01` → `uid:dhamma-1-01/dpd`, the exact entry — a raw `DpdLookup` of
  `dhamma 1.01` finds nothing because of the number), then (2) `DpdLookup` on the
  original query as a last resort.
- **`/suttas_fulltext_search` / `/suttas_contains_search`** fall back to the
  route's own mode (`FulltextMatch` / `ContainsMatch`) on the original query.

This only fires on a **0-hit auto-`UidMatch`**; any query that already returns
≥1 hit, and any **explicitly** requested mode on `/search`, is untouched
(byte-for-byte). So `{"query_text":"dhamma 1.01"}` and
`{"query_text":"dhamma 1.01/dpd"}` to `/dict_combined_search` now return ≥1 hit
(they previously returned `hits: 0`).

## 6. Pagination

Record-based pagination, driven by `page_num` + `page_len`. `hits` is the record
total and is **unchanged** by snippet expansion
or exclusion — it stays constant across pages. `page_len` defaults to 20.

## 7. Response shape (`ApiSearchResult`)

```jsonc
{
  "hits": 42,                       // record total (constant across pages)
  "results": [ /* SearchResult */ ],
  "deconstructor": ["a", "b"]       // Dictionary only; omitted when None
}
```

The API does **not** re-shape, re-highlight, or post-process results. It also
does not compute `show_header` / `find_query` (the app's result list derives
them; they are not stored on `SearchResult`); a client can recompute them from the returned rows.

## 8. The fulltext index opens on the first fulltext query

The fulltext (Tantivy) index is opened by the first query whose mode needs it
(`Fulltext Match`, or `Combined` for suttas and library), not when the server
starts. That query pays the one-off cost of opening the index; later ones do
not. Until then `GET /health` reports `fulltext_searcher_ready: false` (§10),
which is therefore not a fault on a fresh server.

## 9. The same answers from the CLI

The `simsapa` commands of Part A call the same code as the routes, so a
command's `--json` output is the route's response body, byte for byte for the
same request: `simsapa search` is `POST /search` (with the same mode defaults
and the Dictionary `Combined` remap of §4), `simsapa word` is `/word.json`,
`simsapa sutta --content-json` is `/sutta.json`, `simsapa gloss` is
`/gloss_text`, `simsapa lookup` is `/lookup_forms`, and `simsapa health` is
`/health`. Only the named sutta routes' reference auto-detect (§5) has no
command of its own: `simsapa sutta` takes a reference directly.

## 10. The port (default 4848)

The GUI binds to `127.0.0.1:<api_port>`. The port is resolved at startup:

- **Default `4848`.** If the `API_PORT` env var is set to a valid, free port,
  that is used; otherwise the app scans upward from `4848` for the first free
  port (so a second running instance lands on `4849`, etc.).
- The **actual** port chosen is written to `api-port.txt` in `SIMSAPA_DIR`
  (`<SIMSAPA_DIR>/api-port.txt`, single integer, no newline). A client that
  cannot assume `4848` should read this file to discover the live port.

The examples below use `4848`; substitute the value from `api-port.txt` if your
instance differs. A client can confirm the server is up with `GET /` (returns a
small HTML page), fetch the filter option lists with
`GET /sutta_and_dict_search_options`, or get a richer status snapshot with
`GET /health`:

```sh
PORT=$(cat "$SIMSAPA_DIR/api-port.txt")   # or just use 4848

# Liveness check
curl -s "localhost:$PORT/"

# Available filter values: sutta_languages[], dict_languages[], dict_sources[]
curl -s "localhost:$PORT/sutta_and_dict_search_options"

# Diagnostics / health snapshot (JSON)
curl -s "localhost:$PORT/health" | jq .
```

`GET /health` returns a JSON status object — useful for confirming which
databases and dictionaries are live before querying, and whether the Tantivy
fulltext searcher has been initialized (§8):

```jsonc
{
  "app_version": "0.4.4",
  "api_port": 4848,
  "db_paths": {                       // resolved sqlite3 paths actually opened
    "appdata": "…/appdata.sqlite3",
    "dictionaries": "…/dictionaries.sqlite3",
    "dpd": "…/dpd.sqlite3"
  },
  "fulltext_searcher_ready": false,   // see note below
  "fulltext": {                       // per-area detail; see note below
    "state": "could_not_open",        // not_opened_yet | files_not_found | could_not_open | ready
    "message": "The search index could not be opened. This storage location does not support the file locking the search index needs.",
    "failure_count": 6,
    // Per area: its own state and its own sentence. `message` is "" when there
    // is nothing wrong to report.
    "sutta":   { "opened": 0, "dir_present": true, "failed": 2,
                 "state": "could_not_open",
                 "message": "The search index could not be opened. This storage location does not support the file locking the search index needs." },
    "dict":    { "opened": 0, "dir_present": true, "failed": 2, "state": "could_not_open", "message": "…" },
    "library": { "opened": 0, "dir_present": false, "failed": 0, "state": "files_not_found", "message": "" }
  },
  "counts": {                         // row counts in the live DBs
    "suttas": 21359,
    "dict_words": 216009,
    "dpd_headwords": 88864
  },
  "sutta_languages": ["en","pli"],    // same lists as /sutta_and_dict_search_options
  "dict_sources": ["dpd","dppn", …],
  "dpd_version": "v0.4.20260501",     // DPD's own release; null when unreadable
  "sc_data_version": {                // SuttaCentral data; null when not recorded
    "date": "2026-10-17",
    "commit": "2d9ca8da8"
  }
}
```

- **`fulltext_searcher_ready`** means **"at least one index is open"** — i.e.
  whether a `FulltextMatch`/`Combined` query can return anything at all.

  > **This changed.** It previously meant only "the process-global searcher is
  > set", which reported `true` for a searcher that had opened successfully with
  > **zero** indexes. That is not hypothetical: a user on a storage volume
  > without working file locking ran an entire session with every fulltext
  > search silently empty while `/health` said it was ready. A client that
  > treated `true` as "fulltext works" was being told the wrong thing, which is
  > why the meaning was tightened rather than a second field added.

  It is still `false` on a fresh process and flips after the **first**
  `FulltextMatch`/`Combined` query (in the GUI app, also when the app opens
  the index itself) — §8. It now flips to `true` only if that init
  actually opened something. To watch the flip:

  ```sh
  curl -s "localhost:$PORT/health" | jq '.fulltext_searcher_ready'   # false
  curl -s -X POST "localhost:$PORT/suttas_fulltext_search" \
    -H 'Content-Type: application/json' -d '{"query_text":"dukkha"}' >/dev/null
  curl -s "localhost:$PORT/health" | jq '.fulltext_searcher_ready'   # true
  ```

- **`fulltext`** carries the per-area detail behind that boolean, so a client can
  tell *why* it is `false` without reading the log. **Branch on `state`, never on
  a zero `opened`:**

  | `state` | Meaning |
  |---|---|
  | `not_opened_yet` | No searcher built this session. Not measured — **not** "no failures". |
  | `files_not_found` | No index directory, or no per-language index in one. Nothing is broken; nothing has been built or downloaded. |
  | `could_not_open` | Index files are present and every attempt to open one failed. This is a fault. |
  | `ready` | At least one index is open. |

  `opened: 0` for a single area is normal — it usually means the user has not
  downloaded that language. `dir_present` is what separates an absent index tree
  from one that would not open.

  **The top-level `state` answers "does fulltext search work at all", not "will
  a search of area X work".** It is `ready` as soon as anything opened anywhere,
  so a client that cares about one area must read that area's own `state` and
  `message` — the per-area block carries both, and an area can have `opened > 0`
  **and** `failed > 0`, which is working-but-incomplete search and has a sentence
  of its own. This is the same data that the app's own
  empty-results panel shows, so the two cannot drift.

  `message` is plain language and safe to display;
  it deliberately carries no `flock`/`Tantivy`/errno text (that stays in the log
  and in the app's Run Storage Diagnostics report).

- **`counts`** are per-DB row counts. Each is resilient: a real `0` means the DB
  is loaded but empty / not installed (consistent with
  `fulltext_searcher_ready: false`), while `null` means the count query itself
  errored — the rest of `/health` is still returned.
- **`dict_sources`** is the authoritative list of installed dictionaries — check
  it before expecting `/words/<uid>.json` or `dict_dict` filters to resolve a
  given source (the §13.3 verbose-miss `hint` points here for exactly this
  reason).
- **`dpd_version`** is DPD's own release (`db_info.dpd_release_version` in
  `dpd.sqlite3`), the same value `/lookup_forms` and exact `/gloss_text` return.
  **Key caches of DPD-derived answers on it, not on `app_version`**: an app
  update that ships the same DPD release leaves every DPD answer unchanged,
  while a DPD update can change them without an app version a cache would
  notice. The exception is a Simsapa update that **adds fields** to these
  answers (as `form_grammar` and `construction_summary` were added): the values
  stay the same but the shape does not, so also invalidate on an
  `app_version` change if the cache stores whole responses (§19.1). `null`
  (never `""`) when the row cannot be read.
- **`sc_data_version`** is the SuttaCentral data (the `sc-data` repository)
  the suttas were imported from: `date` is its commit's author date
  (`YYYY-MM-DD`, UTC) and `commit` the short commit hash. It is stored in
  `appdata.sqlite3` when the database is built, so it changes when a new
  database is downloaded, not with the app version. `null` for a database built before it was recorded. The
  About window shows the same value, with `dpd_version`.

### 10.1 `simsapa serve`: `--port` and `--host`

`simsapa serve` picks its port the way the GUI does: `--port <n>` if given,
else `API_PORT`, else the first free port from 4848. It writes that port to
`api-port.txt` and prints one line on stdout once it is listening
(`Simsapa API listening on http://127.0.0.1:4848`; with `--json`,
`{"url": …, "host": …, "port": …}`). It stops on Ctrl-C, or on `GET /shutdown`
when bound to a loopback address.

It refuses to start (exit code 3) while the GUI app or another `simsapa serve`
is using the same data folder: point the client at that server instead. The
check reads `api-port.txt` and asks `http://localhost:<port>/health` whether
its `db_paths` are this folder's.

**`--host <address>`** sets the address to bind, an IPv4 or IPv6 address (a
hostname such as `localhost` is rejected, exit code 1). The default
`127.0.0.1` is reachable only from the same machine. Bind `0.0.0.0` (or `::`)
when the client is elsewhere, e.g. outside a Docker container or on another
computer on the network:

```sh
# Inside a container: listen on every interface, publish the port.
docker run -p 4848:4848 … simsapa serve --host 0.0.0.0 --port 4848
curl -s localhost:4848/health | jq .counts     # from the host
```

- **The API has no authentication.** On any non-loopback address `serve`
  prints a warning on stderr before listening:

  ```
  Warning: the Simsapa API is listening on 0.0.0.0:4848 without
  authentication. Anyone who can reach this machine on that port can use it.
  That includes changing this folder's display settings (POST /save_sutta_display_settings)
  and writing to its log (POST /logger). GET /shutdown is not available; stop it with Ctrl-C.
  ```

- **`GET /shutdown` is not mounted** on a non-loopback bind (a `404`), so
  another machine cannot stop the server. The writing routes,
  `POST /save_sutta_display_settings` and `POST /logger`, stay mounted. The
  display settings are stored in `appdata.sqlite3`, so a change made through
  either server is the GUI app's setting too.
- **In-use detection sees only servers reachable on `localhost`.**
  `api-port.txt` holds only the port, and the check calls
  `http://localhost:<port>/health`. That works for `127.0.0.1`, `0.0.0.0` and
  `::`, but a server bound to one specific address (`--host 192.168.1.5`) is not
  seen, and its folder looks free to other `simsapa` commands. Those that write
  are still kept out by the CLI's own write lock, which `serve` holds while it
  runs. The GUI app checks neither, so do not start it on a folder a
  `simsapa serve` or a `simsapa import` is using.

Every `simsapa` command opens the databases afresh (130–250 ms per call for a
one-shot search with the release binaries), so an agent making many queries should start `simsapa serve`
once and use the API rather than run one command per query.

## 11. Response fields (`SearchResult`)

Each element of `results` is a `SearchResult`; the fields most clients use:

| Field | Meaning |
|-------|---------|
| `uid` | Stable id of the row, e.g. `sn56.11/pli/ms`, `dhamma/dpd`, `42/dpd`. Use it to fetch the full text (§13), with the GUI-navigation route `GET /suttas/<uid>`, or to re-query via `Uid Match` / `DPD ID Match`. |
| `schema_name` | Source DB: `appdata`, `dictionaries`, or `dpd`. |
| `table_name` | `suttas`, `dict_words`, `dpd_headwords`, `dpd_roots`, … |
| `title` | Display title (sutta title or dictionary headword). |
| `sutta_ref` | Reference like `SN 56.11` (suttas only). |
| `nikaya`, `author`, `lang` | Collection / author / language code (`pli`, `en`, …). |
| `snippet` | HTML snippet with producer-owned, non-nested `<span class='match'>` highlight spans (see §1). |
| `score`, `rank` | Relevance score / rank where the mode produces them. |
| `is_snippet` | `true` for an expanded per-occurrence row (only when `show_all_snippets` was set); group rows by `uid` to dedupe headers. |

Dictionary responses additionally carry the top-level `deconstructor` array (see
§7) when the DPD deconstructor split the query.

## 12. Usage examples (curl)

The Rocket app is launched via FFI (no standalone route test harness); verify
with `make build -B` plus manual curl against a running app. All examples assume
`PORT=4848`.

### 12.1 Searching the suttas

```sh
# Fulltext (tantivy, stemmed) — the default sutta search.
# Use the named route, or POST /search with "search_area":"Suttas".
curl -s -X POST "localhost:$PORT/suttas_fulltext_search" \
  -H 'Content-Type: application/json' \
  -d '{"query_text":"mindfulness of breathing"}'

# Fulltext with per-occurrence snippets (one result row per match in a sutta).
curl -s -X POST "localhost:$PORT/suttas_fulltext_search" \
  -H 'Content-Type: application/json' \
  -d '{"query_text":"pajahati","show_all_snippets":true}'

# Contains (literal substring): "pajahitvā" is NOT highlighted for "pajahati".
curl -s -X POST "localhost:$PORT/suttas_contains_search" \
  -H 'Content-Type: application/json' -d '{"query_text":"pajahati"}'

# By sutta reference → auto-detected as Uid Match on the named routes.
# Many reference spellings work: "sn56.11", "SN 56.11", "mn44", "dhp182".
curl -s -X POST "localhost:$PORT/suttas_fulltext_search" \
  -H 'Content-Type: application/json' -d '{"query_text":"sn56.11"}'

# Language filter: only English suttas (include=true keeps only "en";
# set suttas_lang_include=false to EXCLUDE "en"). "Language"/"" = no filter.
curl -s -X POST "localhost:$PORT/suttas_fulltext_search" \
  -H 'Content-Type: application/json' \
  -d '{"query_text":"suffering","suttas_lang":"en","suttas_lang_include":true}'

# Pagination + snippet exclusion (drop snippets containing "upādiyati").
# snippet_exclude is a JSON array, not a CSV string.
curl -s -X POST "localhost:$PORT/search" \
  -H 'Content-Type: application/json' \
  -d '{"query_text":"pajahati","search_area":"Suttas","page_num":1,"page_len":10,"snippet_exclude":["upādiyati"]}'

# Explicit mode via /search (strict — no reference→Uid override here):
#   "Title Match"  — match sutta titles only
#   "Uid Match"    — exact uid lookup (pass the uid as query_text)
#   "RegEx Match"  — regular-expression match over the text
curl -s -X POST "localhost:$PORT/search" \
  -H 'Content-Type: application/json' \
  -d '{"query_text":"satipaṭṭhāna","mode":"Title Match","search_area":"Suttas"}'

curl -s -X POST "localhost:$PORT/search" \
  -H 'Content-Type: application/json' \
  -d '{"query_text":"sn56.11/pli/ms","mode":"Uid Match","search_area":"Suttas"}'
```

> Searching the **Library** (imported EPUB/PDF/HTML books) works the same way:
> send `"search_area":"Library"` to `POST /search`. It honours the same
> `suttas_lang*`, pagination, and snippet options as Suttas.

### 12.2 Searching the dictionary

```sh
# DPD general lookup (the dictionary default). /dict_combined_search runs
# DpdLookup (headword/lemma search) and also returns the deconstructor split.
curl -s -X POST "localhost:$PORT/dict_combined_search" \
  -H 'Content-Type: application/json' -d '{"query_text":"dhamma"}'

# Same via the general route (default mode Combined → DpdLookup + deconstructor).
curl -s -X POST "localhost:$PORT/search" \
  -H 'Content-Type: application/json' \
  -d '{"query_text":"dhamma","search_area":"Dictionary"}'

# A compound word — the deconstructor array shows the split (e.g. ["buddha","dhamma"]).
curl -s -X POST "localhost:$PORT/dict_combined_search" \
  -H 'Content-Type: application/json' -d '{"query_text":"buddhadhamma"}'

# By dictionary word UID → auto-detected as Uid Match. The canonical uid from a
# SearchResult's `uid` field works (numeric headword id "34626/dpd" or the
# hyphenated dict_words form "dhamma-1-01/dpd"), AND the human display forms now
# resolve too: the title "dhamma 1.01" and the space-and-dot uid "dhamma 1.01/dpd"
# each return 1 hit (uid "dhamma-1-01/dpd") — they previously returned 0 hits.
curl -s -X POST "localhost:$PORT/dict_combined_search" \
  -H 'Content-Type: application/json' -d '{"query_text":"dhamma-1-01/dpd"}'
curl -s -X POST "localhost:$PORT/dict_combined_search" \
  -H 'Content-Type: application/json' -d '{"query_text":"dhamma 1.01"}'   # now 1 hit

# By DPD headword numeric id, explicitly via /search:
#   "DPD ID Match"  — query_text is the numeric DPD headword id
curl -s -X POST "localhost:$PORT/search" \
  -H 'Content-Type: application/json' \
  -d '{"query_text":"34626","mode":"DPD ID Match","search_area":"Dictionary"}'

# Headword Match — match dictionary headwords across all dictionaries (FTS).
curl -s -X POST "localhost:$PORT/search" \
  -H 'Content-Type: application/json' \
  -d '{"query_text":"nibbāna","mode":"Headword Match","search_area":"Dictionary"}'

# Filter by language and/or source dictionary. dict_lang / dict_dict accept the
# values returned by /sutta_and_dict_search_options; *_include=false EXCLUDES.
# "Language"/"Dictionary"/"" mean "no filter".
curl -s -X POST "localhost:$PORT/dict_combined_search" \
  -H 'Content-Type: application/json' \
  -d '{"query_text":"dhamma","dict_lang":"en","dict_lang_include":true,"dict_dict":"PTS","dict_dict_include":true}'

# After a search, fetch the full word entry as JSON (for glossary export).
# Use the exact uid from the SearchResult; raw / separator, hyphenated stem (§13.3).
curl -s "localhost:$PORT/words/dhamma-1-01/dpd.json"
```

### 12.3 Error handling

```sh
# Unknown mode (or unknown search_area) on /search → HTTP 400.
curl -s -o /dev/null -w '%{http_code}\n' -X POST "localhost:$PORT/search" \
  -H 'Content-Type: application/json' \
  -d '{"query_text":"x","mode":"Nope"}'   # → 400
```

A successful query that simply finds nothing returns HTTP 200 with
`{"hits":0,"results":[]}` (the same shape is returned on an internal query
error, which is logged server-side). If a `Fulltext Match` request unexpectedly
returns empty on a freshly started instance, the Tantivy searcher init is
covered in §8.

## 13. Fetching full text after a search (not a search route)

The search routes return only **snippets**. To read or verify the *full* text of
a result — e.g. to confirm an exact Pāli pāda, or to extract a sentence in
context after a fulltext hit (which may be a false positive, see §2's stemmer
caveat) — use the GET render routes, not the search endpoints. These return rendered HTML; strip the tags to get plain text.

| Route | Returns |
|-------|---------|
| `GET /get_sutta_html_by_uid/<window_id>/<uid..>` | **Full sutta HTML** for a uid. `<window_id>` is any client id (e.g. `web`). Optional `?anchor=<id>` shows reference anchors and jumps to a segment. Applies verse-ref / `/pli/ms` / range normalization (§14.4). **404 on a genuine miss.** |
| `GET /get_word_html_by_uid/<window_id>/<uid..>` | Full dictionary-word entry HTML for a word uid (e.g. `dhamma-1-01/dpd`). Resolves the same tolerant set of uid forms as the JSON route (§13.3); the numeric headword form `34626/dpd` now renders a full page (was blank). **404 on a genuine miss.** |
| `GET /word_html?window_id=<id>&uid=<uid>` | **Query-param twin** of `get_word_html_by_uid`. Same HTML, but the uid is a query parameter so its `/` may be `%2F`-encoded (or raw — encoding-agnostic). Both params required (missing either → HTTP 422). Use when a client must percent-encode the uid. |
| `GET /sutta_html?window_id=<id>&uid=<uid>&[anchor=<id>]` | **Query-param twin** of `get_sutta_html_by_uid`. Same sutta HTML, uid as an encoding-agnostic query parameter (`%2F` or raw `/`). `window_id` + `uid` required, `anchor` optional. Also applies verse-ref / `/pli/ms` / range normalization (§14.4). |
| `GET /sutta.json?uid=<uid>` | The sutta's text as **SuttaCentral (Bilara) JSON**, segment id → text, not HTML. Only for segmented texts; see §13.4. |
| `GET /sutta_bilara.json?uid=<uid>` | The same text **plus its variant readings, comments, glosses and HTML template**, each a segment id → text object; see §13.5. |

Both sutta-HTML routes also accept the optional `layout` / `columns` display
parameters (multi-column side-by-side view) — see §14.5.

```sh
# Full sutta text (e.g. to verify an exact pāda in Snp 1.8, the Metta Sutta):
curl -s "localhost:$PORT/get_sutta_html_by_uid/web/snp1.8/pli/ms"   # then strip HTML

# Full word HTML — path form: pass the uid's / as a raw slash, NOT %2F (see §13.3):
curl -s "localhost:$PORT/get_word_html_by_uid/web/dhamma/ncped"     # then strip HTML

# Same entry via the query-param twins — here %2F IS accepted (curl -G --data-urlencode
# encodes the space and slash for you, proving the route is encoding-agnostic):
curl -s -G "localhost:$PORT/word_html"  --data-urlencode "window_id=web" --data-urlencode "uid=dhamma 1.01/dpd"
curl -s -G "localhost:$PORT/sutta_html" --data-urlencode "window_id=web" --data-urlencode "uid=sn47.8/pli/ms"
```

> **Path routes: pass the uid with raw `/`, not `%2F`.** The two
> `get_*_html_by_uid` routes capture the uid as a multi-segment `<uid..>` path
> parameter; an encoded `%2F` is rejected by Rocket with HTTP 422 (see §13.3).
> **If your client cannot send a raw `/`, use the query-param routes instead**
> — `GET /word_html?…`, `GET /sutta_html?…` (here) and `GET /word.json?uid=`
> (§13.3), which take the uid as a query parameter where `%2F` is valid.

> **404 on a genuine miss.** All four HTML retrieval routes now return **HTTP
> 404** when the uid resolves to nothing (the success body is unchanged). The
> JSON route `word.json` likewise 404s on a miss but keeps a `[]` body for
> back-compat (§13.3).

> **`GET /suttas/<uid>` does NOT return text.** Despite the name it is a
> browser-extension *navigation* route: it pops/raises the Simsapa GUI lookup
> window for that uid and returns only a plain-text "the window should appear"
> message. Use `get_sutta_html_by_uid` for the actual content.

### 13.1 Parallel translations share the sutta number

A Pāli sutta and its English translations share the numeric reference, differing
only in the `lang`/`author` part of the uid:

```
snp1.8/pli/ms      → snp1.8/en/sujato   (also /en/bodhi, /en/thanissaro)
```

So once you have one uid you can fetch a translation by swapping the
`/<lang>/<author>` suffix. Bhikkhu Sujato's HTML interleaves Pāli + English per
segment, so fetching one edition lets you read both side by side. (Watch for
edition variants in the source wording — e.g. the Maṅgala Sutta reads
`pūjaneyyānaṁ` in SuttaCentral/MS but `pūjanīyānaṁ` in some chanting
traditions.)

### 13.2 Verifying dictionary facts (gender, part of speech)

`POST /dict_combined_search` against DPD is the quickest way to confirm a Pāli
word's grammatical gender / part of speech: the returned snippet carries the DPD
grammar label, e.g. `{"query_text":"kaññā","dict_dict":"DPD"}` → a snippet
showing `(fem) young girl … fem`. For the full entry, follow up with
`GET /words/<uid>.json` (§13.3) or `GET /get_word_html_by_uid/...` (above).

### 13.3 Full dictionary-word data as JSON

`GET /words/<uid>.json` returns the **complete** word record as a JSON array
(one element, or empty `[]` when not found) — the structured data behind a
dictionary result, suitable for glossary export. The `.json` suffix is part of
the path but is **optional** — the handler trims a trailing `.json`, so
`/words/dhamma-1-01/dpd` and `/words/dhamma-1-01/dpd.json` are equivalent.

**`GET /word.json?uid=<uid>` is the query-param twin** — same record, same JSON
shape, but the uid is a query parameter instead of a path segment, so its
internal `/` may be sent **either** raw **or** `%2F`-encoded. Use it whenever the
client has to percent-encode the uid; it is the encode-safe alternative to the
path route's raw-slash requirement below. The `uid` param is required (missing →
HTTP 422).

```sh
# Query-param route — encoding-agnostic. curl -G --data-urlencode encodes the
# space (%20) and slash (%2F) for you; raw / works too.
curl -s -G "localhost:$PORT/word.json" --data-urlencode "uid=34626/dpd"
curl -s -G "localhost:$PORT/word.json" --data-urlencode "uid=dhamma 1.01"   # human/display form resolves
```

**Status on a miss — 404, but the body stays `[]`.** A uid that resolves to
nothing now returns **HTTP 404** (both `word.json` and `/words/<uid>.json`); a
hit returns 200. The **body of a default request is unchanged** — a bare JSON
array, `[{…}]` on a hit, `[]` on a miss — so existing clients that ignore the
status code and just parse the array keep working byte-for-byte. Use the status
code to distinguish hit from miss without inspecting the array length.

**`?verbose=1` opt-in envelope.** Add `verbose=1` to wrap the result in a
diagnostic object instead of the bare array (default stays a bare array — opt-in
only). A **hit** returns
`{"found":true,"canonical_uid":"dhamma-1-01/dpd","query_uid":"dhamma 1.01","result":{…single record…}}`
— note `result` is the single record **object**, and `canonical_uid` reports the
resolved canonical uid for the form you sent. A **miss** (still HTTP 404) returns
`{"found":false,"canonical_uid":null,"query_uid":"nope/dpd","hint":"no word for this uid; tried nope/dpd. Is the source dict installed? See /health."}`.
(The hint points at `GET /health` — a diagnostics route that lists the installed
dictionaries among other things; see §10.)

```sh
# 404-on-miss, body still []:
curl -s -o /dev/null -w 'hit  -> %{http_code}\n' -G "localhost:$PORT/word.json" --data-urlencode "uid=dhamma-1-01/dpd"  # 200
curl -s -o /dev/null -w 'miss -> %{http_code}\n' -G "localhost:$PORT/word.json" --data-urlencode "uid=nope/dpd"         # 404
curl -s            -G "localhost:$PORT/word.json" --data-urlencode "uid=nope/dpd"                                       # => []

# Verbose envelope (hit shows canonical_uid; miss shows found:false + hint):
curl -s -G "localhost:$PORT/word.json" --data-urlencode "uid=dhamma 1.01" --data-urlencode "verbose=1" | jq .
curl -s -G "localhost:$PORT/word.json" --data-urlencode "uid=nope/dpd"    --data-urlencode "verbose=1" | jq .
```

**On the path route, pass the uid's internal `/` as a raw, literal `/`** — do
**not** percent-encode it as `%2F`. The path routes capture the uid as a
multi-segment trailing path parameter (`<uid..>`, a `PathBuf`); Rocket rejects an
encoded `%2F` in such a segment as a path-traversal safeguard and returns
**HTTP 422**. Only genuinely-unsafe characters need encoding — e.g. a space as
`%20`. (The sutta route `GET /get_sutta_html_by_uid/<window_id>/<uid..>` works
the same way: its `sn22.59/pli/ms`-style uids are passed with raw slashes.) The
`/word.json?uid=` query route above has no such restriction.

The JSON and HTML word routes resolve the same tolerant set of uid forms — human/display,
hyphenated, and numeric forms all return a record (the spaced/human forms
previously returned `200 []`). It probes these tables in order, returning the
first match serialized as-is:

| Probe order | uid shape | Source table | Distinguishing field | Verified example uid |
|---|-----------|-------------|----------------------|----------------------|
| 1 | `{bold}/{ref_code}` (overlaps dict_word namespace) | `bold_definitions` (dpd.sqlite3) | `ref_code` | commentary bold-definition uid |
| 2 | ends `…/dpd` and numeric stem | `dpd_headwords` (dpd.sqlite3) | `lemma_1` | `34626/dpd` |
| 3 | `√<root>/dpd` | `dpd_roots` (dpd.sqlite3) | — | `√kar/dpd` |
| 4 | anything else (+ sanitize / display-form normalization) | `dict_words` (appdata / dictionaries) | `dict_label` | `dhamma-1-01/dpd`, `dhammamaccharī/dpd` |

**Two-lane invariant.** The numeric form (`34626/dpd`) resolves to a
`dpd_headwords` row (has `lemma_1`); the hyphenated form (`dhamma-1-01/dpd`)
resolves to a `dict_words` row (has `dict_label`). These are **different records
for the same word** — pick the lane whose fields you need.

```sh
# DPD headword by numeric id (raw / separator)
curl -s "localhost:$PORT/words/34626/dpd.json"

# DPD root
curl -s "localhost:$PORT/words/√kar/dpd.json"

# A dict_words entry — note the HYPHENATED stem (dhamma-1-01, not "dhamma 1.01")
curl -s "localhost:$PORT/words/dhamma-1-01/dpd.json"
```

The element shape is the serialized DB row (DPD headword JSON, DPD root JSON, or
a `dict_words` entry) — not the `SearchResult` of §11.

A DPD headword record (the numeric `<id>/dpd` lane only) also carries
`construction_summary`, the construction as DPD's entry displays it
(`49236/dpd` *bhaṇāmase*: `"√bhaṇ + a + mase"`, where `construction` is
`bhaṇa + mase`); `""` when the entry shows none, `null` when the installed
`dpd.sqlite3` predates the column (see §19.1). The
`dict_words` lane and roots do not have it. Keys are alphabetical, so the
added key moves no other.

> **Gotchas — read before using `/words/<uid>.json`:**
>
> 1. **Prefer a uid discovered from a search; the resolver has some fuzzy
>    fallback but it is not exhaustive.** Run a search
>    (e.g. `POST /dict_combined_search`, §12.2) and copy the `uid` field from a
>    `SearchResult` verbatim — that is still the most reliable way to get a uid
>    that resolves, especially for less common dictionaries.
> 2. **The numbered-headword uid is hyphenated, but the display-title form now
>    resolves too.** A DPD result whose `title` shows `dhamma 1.01` has the
>    canonical uid **`34626/dpd`** (numeric) or, in `dict_words`,
>    **`dhamma-1-01/dpd`** (hyphens, no space or dot). The space-and-dot
>    display-title forms — `dhamma 1.01/dpd` **and** the bare `dhamma 1.01` —
>    now resolve to the same record (HTTP 200 with data, no longer `[]`); the
>    resolver gained a display-title fallback. This applies to **both** the path
>    route (`/words/<uid>.json`) and the query route (`/word.json?uid=`).
>    (Encode the space as `%20` on either; the query route also accepts the `/`
>    as `%2F`.)
> 3. **A miss is now `[]` with HTTP 404 (not 200).** An empty `[]` body still
>    means "no such uid", but the status code is now **404** — check it to tell a
>    miss from a hit without inspecting array length (or use `verbose=1` for
>    `found:false` + a `hint`). With the tolerant resolver (gotcha 2) the common
>    remaining causes are a uid form even the fallback doesn't cover, or a
>    dictionary that is not installed — e.g. `dhamma/ncped` 404s here because
>    `ncped` is not in `dict_sources` (check `GET /sutta_and_dict_search_options`;
>    this build has only `dpd`, `dppn`). Confirm the source dict exists first.
> 4. **JSON and HTML routes now resolve the same forms.** Previously the HTML
>    route rendered uids the JSON route returned `[]` for; the resolver fix
>    closed that gap, so `word.json` / `/words/<uid>.json` and the HTML routes
>    accept the same human/hyphenated/numeric forms. (The numeric headword form
>    `34626/dpd`, which used to render a *blank* HTML page, now renders the full
>    entry.) Use the JSON route when you need structured fields, the HTML route
>    when you need rendered markup — not as fallbacks for each other.

### 13.4 A sutta as SuttaCentral (Bilara) JSON: `GET /sutta.json`

`GET /sutta.json?uid=<uid>` returns a sutta's text in SuttaCentral's Bilara
format: one JSON object mapping each **segment id** to that segment's text.
The body is the `suttas.content_json` column **verbatim**, so the segments come
in the order of the SuttaCentral file, and the whitespace is the stored file's.

```sh
curl -s -G "localhost:$PORT/sutta.json" --data-urlencode "uid=mn1/pli/ms" \
  | jq 'to_entries[:3] | from_entries'
```

```json
{
  "mn1:0.1": "Majjhima Nikāya 1 ",
  "mn1:0.2": "Mūlapariyāyasutta ",
  "mn1:1.1": "Evaṁ me sutaṁ—"
}
```

The whole `mn1/pli/ms` object has 334 segments. A translation such as
`mn1/en/sujato` has the same segment ids, so the Pāli and an English
translation can be zipped by key.

- **Response `200`:** `Content-Type: application/json`, the Bilara object as
  the body. The `X-Simsapa-Sutta-Uid` header names the record that answered,
  which can differ from the request (see below). Read it with `curl -sD - …`.
- **Order: never re-sort `content_json`.** The key order *is* the order of
  the paragraphs in the text, and the segment ids do not sort into it. Sorted,
  `mn1:10.1` follows `mn1:1.6`, and 326 of the 334 segments of `mn1/pli/ms`
  move. The server returns the stored text without parsing it. A client
  must read it with a parser that keeps key order (`jq` without `-S` or
  `keys`, Python's `json`, JavaScript's `JSON.parse`) and must not pass it
  through a sorted or unordered map.
- **Which texts have it.** Only segmented texts: the SuttaCentral Pāli
  (`/pli/ms`) and the Bilara translations (e.g. `/en/sujato`, `/en/brahmali`).
  `pli/cst` texts and most other translations are stored as HTML; for those use
  `/sutta_html` (§13). `GET /translations_for_sutta?uid=` reports
  `has_content_json` for each text sharing a reference (§14.5).
- **uid resolution**:
  1. a verse reference is converted (`dhp182` → `dhp179-196/pli/ms`, §14.4);
  2. the uid is read exactly;
  3. a bare code with no `/lang/author` reads `<code>/pli/ms` (`mn1` →
     `mn1/pli/ms`);
  4. a single reference inside a stored range reads the range, with the same
     `/lang/author` (`sn45.92/en/sujato` → `sn45.92-95/en/sujato`).

  **It never substitutes a different text.** The HTML routes fall back from a
  missing translation to the Pāli (§14.4); this route does not, so
  `mn1/en/nosuchauthor` is a `404`, not the Pāli.
- **Errors**, both `404` with a JSON body (`reason` is stable; the `error`
  text is for people):

  | Case | Body |
  |---|---|
  | No sutta for the uid | `{"error": "sutta not found: <uid>", "reason": "not_found", "uid": "<uid as sent>"}` |
  | The sutta is stored as HTML | `{"error": "<uid> has no SuttaCentral JSON content; …", "reason": "no_content_json", "uid": "<resolved uid>"}` |

  A missing `uid` parameter is Rocket's own `422`.

```sh
# A verse reference, and the record that answered:
curl -s -D - -o /dev/null -G "localhost:$PORT/sutta.json" --data-urlencode "uid=dhp182" \
  | grep -i x-simsapa-sutta-uid
# x-simsapa-sutta-uid: dhp179-196/pli/ms

# A text stored as HTML:
curl -s -G "localhost:$PORT/sutta.json" --data-urlencode "uid=dn1/pli/cst" | jq .reason
# "no_content_json"
```

This route serves the text only. The variant readings, comments, glosses and
HTML template SuttaCentral publishes beside it are served by
`/sutta_bilara.json` (§13.5).

### 13.5 A sutta with its variant readings, comments and glosses: `GET /sutta_bilara.json`

SuttaCentral publishes a segmented text as several files keyed by the same
segment ids: the text (`root` or `translation`), the variant readings
(`variant`), the translator's notes (`comment`), the HTML markup (`html`) and,
for some texts, word glosses. `GET /sutta_bilara.json?uid=<uid>` returns all of
them for one sutta in a single object:

```sh
curl -s -G "localhost:$PORT/sutta_bilara.json" --data-urlencode "uid=mn1/en/sujato" \
  | jq '{uid, language, source_uid,
         text: .content["mn1:3.2"], html: .html_template["mn1:3.2"],
         variant: {uid: .variants.uid, reading: .variants.content["mn1:3.2"]},
         comment: (.comments.content["mn1:3.2"] | .[:60]),
         glosses}'
```

```json
{
  "uid": "mn1/en/sujato",
  "language": "en",
  "source_uid": "sujato",
  "text": "They perceive earth as earth. ",
  "html": "{}",
  "variant": { "uid": "mn1/pli/ms", "reading": "pathaviṁ → paṭhaviṁ (bj, sya-all, km, pts1ed) " },
  "comment": "Although their perception (<i lang='pi' translate='no'>saññā",
  "glosses": null
}
```

The response object:

| Key | Value | Source |
|---|---|---|
| `uid`, `language`, `source_uid` | The sutta that answered (also in `X-Simsapa-Sutta-Uid`), its language and author / edition code | `suttas` |
| `content` | The text, segment id → text: exactly the `/sutta.json` body (§13.4) | `suttas.content_json` |
| `html_template` | Segment id → HTML markup, with `{}` where the segment's text goes; `null` when none is stored | `suttas.content_json_tmpl` |
| `variants` | Variant readings, `null` when none (see below) | `sutta_variants` |
| `comments` | The translator's notes (HTML), `null` when none | `sutta_comments` |
| `glosses` | Word glosses, `null` when none | `sutta_glosses` |

`variants`, `comments` and `glosses` are each `null` or a record
`{"uid", "language", "source_uid", "content"}`, where `uid` names the text the
record belongs to and `content` is its segment id → text object. A record covers
only the segments that have an entry, so look it up by the segment ids of
`content`, never iterate it as if it were the text.

- **Variant readings belong to the Pāli.** SuttaCentral publishes them for the
  root text (`<ref>/pli/ms`), as `reading → alternative (editions)`. For a
  translation the route returns the variant readings of the Pāli text with the
  same reference, and `variants.uid` names it (`mn1/pli/ms` for
  `mn1/en/sujato`). The segment ids are the same, so they apply to the
  translation's segments all the same.
- **Comments belong to the translation.** They are the translator's notes
  (Bhante Sujato's, Bhante Brahmali's), so a Pāli text has `comments: null`.
- **Glosses**: the `sutta_glosses` table is in the schema and the reader
  displays it, but no gloss records are in the shipped database today, so
  `glosses` is `null`.
- **Order.** Every segment object is the stored column **verbatim**, embedded
  unparsed (as a `serde_json` `RawValue`), so the segment order is the file's,
  as with `/sutta.json`. The rules of §13.4 for reading it apply to each one.
- **uid resolution, header and errors are those of `/sutta.json`** (§13.4):
  strict resolution, never a fallback from a translation to the Pāli, the
  resolved uid in `X-Simsapa-Sutta-Uid`, and the same two `404` bodies
  (`reason`: `not_found`, `no_content_json`). A text stored as HTML has none of
  these records.
- A stored record that is not valid JSON is a data defect: it is logged and
  that key is `null`; the rest of the response is served.

## 14. Complete route reference

Every route of the GUI app's server. **`simsapa serve`
serves §14.1 except the three rows marked *GUI only (AI)*, and of §14.3 only
`/`, `/health`, `/logger` and `/shutdown`** (the last only on a loopback bind,
§10.1). All of §14.2 is GUI only. Routes detailed earlier are cross-referenced;
the rest are listed here with their purpose. Many of the GUI-navigation routes are **side-effecting**: they fire a
callback into the running GUI (open a window/tab, navigate, toggle a
mode) and return only an HTTP `Status` or a short plain-text message — they do
**not** return content. They exist for the browser extension and in-app WebEngine
views, not for headless data retrieval.

### 14.1 Search & data retrieval (return JSON / HTML / text)

| Method · Path | Purpose | Details |
|---|---|---|
| `POST /search` | General search, any mode + area | §1–§12 |
| `POST /suttas_fulltext_search` | Suttas, FulltextMatch (tantivy) / Uid auto-detect | §1, §5, §12.1 |
| `POST /suttas_contains_search` | Suttas, ContainsMatch (literal) / Uid auto-detect | §1, §12.1 |
| `POST /dict_combined_search` | Dictionary, DpdLookup + deconstructor / Uid auto-detect | §1, §12.2 |
| `POST /reference_search` | Sutta reference table lookup: PTS volume/page (range-matched), DPR reference, Pāli title or SuttaCentral reference → every reference style of the matching suttas, as `{hits, results[]}` capped by `limit`. 400 on unknown `field` | §17 |
| `GET /sutta_and_dict_search_options` | Filter option lists (`sutta_languages[]`, `dict_languages[]`, `dict_sources[]`) | §10; struct `SearchOptions` §15 |
| `GET /get_sutta_html_by_uid/<window_id>/<uid..>?<anchor>&<layout>&<columns>` | Full rendered sutta HTML (text retrieval); 404 on miss; optional display params | §13, §14.5 |
| `GET /sutta_html?window_id=<id>&uid=<uid>&[anchor=<id>]&[layout=…]&[columns=…]` | Query-param twin of `get_sutta_html_by_uid`; uid encoding-agnostic (`%2F` ok); 404 on miss; optional display params | §13, §14.5 |
| `GET /sutta.json?uid=<uid>` | The sutta's text as SuttaCentral (Bilara) JSON, segment id → text, the stored `content_json` verbatim; the resolved uid is in `X-Simsapa-Sutta-Uid`; strict uid resolution; 404 + JSON `reason` (`not_found` / `no_content_json`) | §13.4 |
| `GET /sutta_bilara.json?uid=<uid>` | The `/sutta.json` text with its companion Bilara records in one object: `content`, `html_template`, `variants` (for a translation, the Pāli root's), `comments`, `glosses`; each segment object verbatim. Same uid resolution, header and 404 bodies as `/sutta.json` | §13.5 |
| `GET /sutta_content_block?uid=<uid>&[layout=…]&[columns=…]&[show_references=…]` | Just the sutta content-block HTML (no page chrome) for in-page layout/column re-renders; 400 bad layout, 404 unknown uid/column; 200 carries the resolved column list in the `X-SSP-Columns` header | §14.5 |
| `GET /translations_for_sutta?uid=<uid>` | JSON array of the other texts sharing the sutta's reference (column-bar dropdowns): `item_uid`, `sutta_title`, `sutta_ref`, `language`, `author`, `has_content_json` | §14.5 |
| `POST /save_sutta_display_settings` | Persist the `sutta_display` defaults (cogwheel menu "Save as default"). Body: `SuttaDisplayDefaults` JSON; 200 on success, 400/422 on malformed body | §14.5 |
| `GET /get_word_html_by_uid/<window_id>/<uid..>` | Full rendered dictionary-word HTML; 404 on miss | §13 |
| `GET /word_html?window_id=<id>&uid=<uid>` | Query-param twin of `get_word_html_by_uid`; uid encoding-agnostic (`%2F` ok); 404 on miss | §13 |
| `GET /words/<uid>.json` | Full dictionary-word record as JSON (path form; raw `/` only); 404 + `[]` on miss | §13.3 |
| `GET /word.json?uid=<uid>&[verbose=1]` | Full dictionary-word record as JSON (query form; uid encoding-agnostic). Default bare array, 404 + `[]` on miss; `verbose=1` → diagnostic envelope. A DPD headword (`<id>/dpd`) also carries `construction_summary` | §13.3 |
| `POST /lookup_forms` | Exact DPD form lookup: up to 5,000 forms → one result per form, in input order, with every headword and split of the form's DPD `lookup` row and nothing chosen, DPD's parses of the form (`form_grammar_raw`, per headword `form_grammar`, per split component its own row's) and each headword's `root_base` / `root_sign` / `construction_summary`; `dpd_version`. 400 / 500 with JSON `{ "error" }` | §19.1 |
| `POST /gloss_text` | Synchronously gloss `\n\n`-separated paragraphs → `AllParagraphsProcessingResult` JSON (full `ProcessedWord` per word, incl. grouped deconstruction fields). With `options.exact: true`, every token in order with its exact lookup instead. 400 on bad body; empty text → 200 empty paragraphs | §16.1, §19.2 |
| `GET /word_selection_ws` | *GUI only (AI).* WebSocket word-selection run over glossed paragraphs; streams `status` / `error` / final `result` messages; accepts `cancel` | §16.2 |
| `POST /set_ai_provider_key` | *GUI only (AI).* Set a provider's API key + enable it (and optionally a model), so the WebSocket engine — which uses the app's **saved** settings — can reach it. Never echoes the key | §16.4 |
| `GET /ai_provider_key?provider=Gemini` | *GUI only (AI).* Read a provider's saved API key + enabled state (localhost demo prefill). Returns the key value | §16.4 |
| `GET /get_book_spine_item_html_by_uid/<window_id>/<spine_item_uid..>` | Full rendered Library-book chapter HTML, by spine-item uid | — |
| `GET /book_pages/<book_uid>/<resource_path..>` | Rendered Library-book page HTML, by in-book resource path | — |
| `GET /navigation_timeline?[window_id=<id>]` | The Timeline page of the Sutta Navigation window: every node with a context note on one time axis, as a full HTML page with the find bar. With no navigation tree in the database, the controls and "No timeline data" | — |
| `GET /sutta_titles_flat_completion_list` | Autocomplete list of sutta titles. **Placeholder — returns `[]`** (the extension uses a bundled list) | — |
| `GET /dict_words_flat_completion_list` | Autocomplete list of dictionary words. **Placeholder — returns `[]`** | — |

### 14.2 GUI navigation (side-effecting; open/navigate windows; GUI only)

| Method · Path | Purpose |
|---|---|
| `GET /suttas/<uid..>` | Open a sutta in the GUI **lookup window**. Returns a plain-text "window should appear" message, *not* the text (use `get_sutta_html_by_uid`). 404 if not found. Accepts verse refs and range uids (see §14.4). |
| `GET /open_sutta_window/<uid..>` | Open a sutta in a **new** sutta-search window. Returns `Status` (404 if not found). |
| `GET /open_sutta_tab/<window_id>/<uid..>?<anchor>` | Open a sutta in a **new tab** of an existing window `window_id`, optionally scrolled to `anchor`. |
| `POST /open_book_page_tab/<window_id>` | Open a Library-book page (parsed from a `/book_pages/...` URL, with optional `#anchor`) in a new tab. Body: `BookPageRequest` (§15). |
| `GET /prev_sutta/<window_id>/<current_sutta_uid..>` | Navigate `window_id` to the previous sutta. |
| `GET /next_sutta/<window_id>/<current_sutta_uid..>` | Navigate `window_id` to the next sutta. |
| `GET /prev_chapter/<window_id>/<current_spine_item_uid..>` | Navigate `window_id` to the previous Library-book chapter. |
| `GET /next_chapter/<window_id>/<current_spine_item_uid..>` | Navigate `window_id` to the next Library-book chapter. |
| `GET /show_toc_tab/<window_id>/<spine_item_uid..>` | Show the sidebar's **TOC** tab in `window_id` with the entry for `spine_item_uid` revealed (ancestors expanded, entry selected and scrolled into view). Called by the in-page TOC button on book chapter pages. Always `200` — it hands off to the GUI and does not read the DB. |
| `GET /open_sutta_navigation?node_uid=<uid>` | Open or raise the **Sutta Navigation** window with the node's level entered (the button on a Timeline entry). 404 when the tree has no such node. |
| `GET /lookup_window_query/<text>` | Open the word-lookup window and run a query (text in the path). |
| `POST /lookup_window_query` | Open the word-lookup window. If `query_text` is a word **uid** (contains `/`) it opens that entry directly (dict_words → DPD headword fallback); otherwise it runs a lookup search. Body: `LookupWindowRequest` (§15). |
| `GET /summary_query/<window_id>/<text>` | Run a summary query in `window_id`. |
| `POST /dppn_lookup` | Look up a proper name (DPPN — *Dictionary of Pāli Proper Names*) in `window_id`. Body: `DppnLookupRequest` (§15). |
| `POST /sutta_menu_action` | Trigger a sutta context-menu action (selected-text action) in `window_id`. Body: `SuttaMenuRequest` (§15). |
| `GET /toggle_reading_mode/<window_id>/<is_active>` | Toggle reading mode on/off (`is_active` = `true`/`false`) in `window_id`. |

### 14.3 Static assets, resources & utility

| Method · Path | Purpose |
|---|---|
| `GET /` | Liveness — minimal HTML page (see §10). |
| `GET /health` | JSON diagnostics snapshot: `app_version`, `api_port`, `db_paths`, `fulltext_searcher_ready`, `fulltext` (per-area index state), `counts`, `sutta_languages`, `dict_sources` (see §10). |
| `GET /shutdown` | Shut the webserver down; the GUI app calls it when it quits. Not mounted when `simsapa serve` binds a non-loopback address (§10.1). |
| `GET /app-assets-list` | *GUI only.* Debug HTML listing of the SIMSAPA_DIR and internal-storage directory trees. |
| `GET /assets/<path..>` | *GUI only (static asset).* Serve a bundled static asset (CSS/JS/fonts/images/pdf-viewer) from the embedded `assets/` dir. |
| `GET /favicon.ico` | *GUI only (static asset).* Serve the app icon as the favicon. |
| `GET /book_resources/<book_uid>/<path..>` | *GUI only (static asset).* Serve a binary resource (image/css/font/pdf) imported with a Library book, from the DB. |
| `GET /dict_resources/<dict_id>/<path..>` | *GUI only (static asset).* Serve a binary resource imported with a StarDict dictionary, keyed by numeric `dict_id`. |
| `GET /get_pdf_viewer/<book_uid>` | *GUI only (static asset).* Redirect/loader HTML that opens the bundled PDF.js viewer pointed at a book's `document.pdf` (browser testing). |
| `POST /logger` | Write a message to the app log. Body: `LoggerRequest` (§15). |
| `POST /copy_to_clipboard` | *GUI only.* Copy text to the system clipboard (`text/plain`). Body: `CopyToClipboardRequest` (§15). |
| `POST /open_external_url` | *GUI only.* Open a URL in the system browser. Body: `OpenExternalUrlRequest` (§15). |

### 14.4 Shared uid handling (sutta routes)

The sutta GUI-open routes (`/suttas`, `/open_sutta_window`, `/open_sutta_tab`)
**and** the sutta HTML retrieval routes (`get_sutta_html_by_uid`, `sutta_html`)
share two helpers, so they accept more than a literal stored uid:

- **Verse-reference conversion** (`convert_verse_ref_to_sutta_uid`): e.g.
  `thag179/pli/ms` → `thag2.30/pli/ms`, `dhp34` → `dhp33-43/pli/ms`. A bare code
  with no `/lang/author` defaults to `/pli/ms`.
- **Fallback + range lookup** (`lookup_sutta_with_fallback`): if the exact uid
  isn't found it retries the `…/pli/ms` edition, then looks for a stored **range**
  sutta containing the reference (e.g. `sn45.92/pli/ms` → `sn45.92-95/pli/ms`).

`GET /sutta.json` (§13.4) and `GET /sutta_bilara.json` (§13.5) use the
verse-reference conversion but **not**
`lookup_sutta_with_fallback`: it never falls back from a translation to the
Pāli, and it keeps a range hit only when the `/lang/author` matches.

The search routes' reference auto-detect (§5) is a different mechanism: it
rewrites the query string into a `uid:` field query.

### 14.5 Sutta display parameters and routes (multi-column view)

The multi-translation side-by-side view adds optional display parameters to
the sutta render routes plus three dedicated routes. Shared parameter
semantics (absent parameters fall back to the display defaults saved in the
app's settings):

- `layout` — `lines` / `linebyline` (interleaved) or `columns` / `sidebyside`
  (one flex column per text). Unknown value → **HTTP 400** with a message.
- `columns` — `|`-separated ordered column sutta uids (percent-encode each uid
  if needed; the query value is decoded once). Default when absent: the opened
  sutta + its Pāli counterpart. In Lines mode, non-segmented columns (no
  `content_json`) are **silently dropped** at options resolution; in Columns
  mode a non-segmented column switches the render to the unaligned
  block-columns fallback.
- `show_references` — `true`/`false`, render per-segment reference anchors
  (`sutta_content_block` only; the full-page routes derive it from `anchor`).

Routes:

- `GET /get_sutta_html_by_uid/…?layout=…&columns=…` and
  `GET /sutta_html?…&layout=…&columns=…` — full page with the display
  overrides applied (param parity between the twins). The page injects a
  `SUTTA_DISPLAY` JS object (resolved layout, column `{uid, label}` list,
  `show_references`) next to `SUTTA_UID` for the in-page menu/column bar.
  **Error parity with `/sutta_content_block`:** an unknown `columns` uid →
  **404** with the message ("Unknown column sutta uid: …"), other render
  errors → 500 (the same mapping as there); an
  unknown *sutta* uid stays the blank page + 404.
- `GET /sutta_content_block?uid=…&layout=…&columns=…&show_references=…` —
  returns only the `<div class='suttacentral bilara-text …'>` content block
  (incl. the Columns-mode header row), no page chrome / `window_id`; used by
  the in-page cogwheel menu and column bar to swap `#ssp_content` live.
  400 on a bad `layout`, 404 with a message on an unknown sutta or column uid.
  The 200 response carries an **`X-SSP-Columns` header**: the server-resolved
  column list (after the Lines-mode non-segmented drop and the Repeat-Pāli
  arrangement) as a percent-encoded JSON array of
  `{uid, label, author, is_pali}` — same shape as `SUTTA_DISPLAY.columns`;
  decode with `decodeURIComponent`. The client adopts it after the swap.
- `GET /translations_for_sutta?uid=…` — JSON array of the other texts sharing
  the sutta's reference, each entry `{item_uid, table_name, sutta_title,
  sutta_ref, language, author, has_content_json}`. `has_content_json: false`
  means the text is only available in the Columns layout.
- `POST /save_sutta_display_settings` — body is a `SuttaDisplayDefaults` JSON
  object (`layout`, `pali_font`, `translation_font`, `author_ink_colors`,
  `author_bg_colors`); persists through `AppData` and refreshes the settings
  cache, so a following render without params uses the new defaults.

```sh
# Three-column aligned view (every column segmented):
curl -s -G "localhost:$PORT/sutta_content_block" \
  --data-urlencode "uid=an4.1/en/sujato" \
  --data-urlencode "layout=columns" \
  --data-urlencode "columns=an4.1/en/sujato|an4.1/pli/ms|an4.1/en/kovilo"

# Which texts can be columns (has_content_json)?
curl -s -G "localhost:$PORT/translations_for_sutta" --data-urlencode "uid=an4.1/en/sujato"

# Save new display defaults:
curl -s -X POST "localhost:$PORT/save_sutta_display_settings" \
  -H "Content-Type: application/json" -d '{"layout":"sidebyside"}'
```

## 15. Other request / response structs

The four search routes share `ApiSearchRequest` / `ApiSearchResult` (§2, §7) and
`SearchResult` rows (§11). The remaining routes use these smaller structs;
all are plain JSON objects.

```rust
// GET /sutta_and_dict_search_options  → response
struct SearchOptions {
    sutta_languages: Vec<String>,
    dict_languages: Vec<String>,
    dict_sources: Vec<String>,
}

// POST /lookup_window_query  ← request
struct LookupWindowRequest { query_text: String }

// POST /reference_search  ← request (response rows: §17)
struct ReferenceSearchRequest { query_text: String, field: Option<String>, limit: Option<usize> }

// POST /sutta_menu_action  ← request
struct SuttaMenuRequest { window_id: String, action: String, text: String }

// POST /dppn_lookup  ← request
struct DppnLookupRequest { window_id: String, query: String }

// POST /open_book_page_tab/<window_id>  ← request
struct BookPageRequest { book_page_url: String }  // e.g. "/book_pages/<uid>/ch1.xhtml#sec2"

// POST /logger  ← request
struct LoggerRequest { log_level: String, msg: String }  // log_level: info|warn|error|profile

// POST /copy_to_clipboard  ← request
struct CopyToClipboardRequest { text: String }

// POST /open_external_url  ← request
struct OpenExternalUrlRequest { url: String }
```

`/words/<uid>.json` returns `Vec<serde_json::Value>` (the raw DB row, plus
`construction_summary` on a DPD headword, §13.3); the
two `*_flat_completion_list` routes return `Vec<String>` (currently empty). The
HTML routes (`get_*_html_by_uid`, `book_pages`, `get_pdf_viewer`, `index`,
`app-assets-list`) return `RawHtml<String>`; the asset/resource routes return raw
bytes with a `Content-Type`; the side-effecting GUI routes return a bare
`Status`.

```sh
# Example: open a sutta entry directly in the lookup window by word uid
curl -s -X POST "localhost:$PORT/lookup_window_query" \
  -H 'Content-Type: application/json' -d '{"query_text":"dhamma-1-01/dpd"}'

# Example: look up a proper name (DPPN) in window "web"
curl -s -X POST "localhost:$PORT/dppn_lookup" \
  -H 'Content-Type: application/json' -d '{"window_id":"web","query":"Anuruddha"}'
```

## 16. Gloss pipeline routes: `POST /gloss_text` + the word-selection WebSocket

These two routes expose the app's Gloss tab pipeline so an external client can
build a vocabulary-table UI: **`POST /gloss_text`** turns Pāli text into a
per-word structured result (senses + deconstructor break-downs), and the
**`GET /word_selection_ws`** WebSocket runs the AI "Word Selection" over that
result — picking each ambiguous word's sense and each compound's break-down —
while streaming progress. `/gloss_text` is a shared route, also served by
`simsapa serve` and printed by `simsapa gloss --json`; it is the same glossing
the app's Gloss tab does. The WebSocket is GUI only. Without the GUI, the
calling agent makes the selections itself (§16.5).

### 16.1 `POST /gloss_text`

Synchronous JSON glossing of one or more `\n\n`-separated paragraphs.

- **Request:** `{ "text": "<paragraphs, \n\n separated>", "options"?: {…} }`.
  `options` (all optional): `no_duplicates_globally` (bool, default `false`),
  `skip_common` (bool, default `false`), `common_words` (string[], default `[]`),
  `exact` (bool, default `false`).
- **`exact: true`** switches to a different response, `ExactGlossResult`: one
  token per word of each paragraph, in order, each with its exact DPD lookup,
  and nothing deduplicated, skipped or chosen. Its shape, tokenisation and an
  example are §19.2. The other three options do not apply to it, and setting
  any of them to a non-default value with `exact: true` is a `400`. Without
  `exact`, or with `false`, the response below is unchanged.
- **Stateless per call.** Unlike in-app incremental glossing, the route keeps no
  cross-call dedup state; `no_duplicates_globally` applies **within the one
  submitted text only** (the `existing_global_*` carry fields are zeroed).
- **Response:** `AllParagraphsProcessingResult` —
  `{ success, paragraphs[], global_unrecognized_words[], updated_global_stems{} }`.
  Each `paragraphs[]` is `{ paragraph_index, unrecognized_words[], words_data[] }`;
  each `words_data[]` is a full `ProcessedWord`.
- **Errors:** 400 on an unparseable body, with Rocket's own HTML error page
  (the mode is not known until the body parses); empty text → 200 with
  `paragraphs: []`. A body over the server's 2 MiB `string` limit is also
  Rocket's own non-JSON `400`. A database error in the default mode is a `200`
  with `success: false`. In exact mode the errors found after parsing carry a
  JSON `{ "error": "…" }` body: `400` for an option it rejects
  (`"exact mode does not support these options: skip_common"`), `500` for a
  database error.

**`ProcessedWord` — the grouped deconstruction fields.** The key fields for a compound-aware client:

| Field | Meaning |
|-------|---------|
| `stem` / `original_word` | The dictionary stem and the surface word as it appeared. |
| `results[]` | Flat, deduplicated result list (direct results first, then deconstructor-derived in first-seen order). Each has `uid`, `summary`, etc. |
| `direct_uids[]` | Uids found via direct / uid / i2h / stem match. **Empty ⇒ the word is deconstructor-resolved** (rendered as component sub-rows). |
| `deconstructions[]` | Break-downs: `{ words_joined, components: [{ word, result_uids[] }] }`. A result uid may appear in `direct_uids` **and** in several break-downs (many-to-many). |
| `selected_index` | Chosen sense index into the direct results (cases (a)/(b)). |
| `selected_deconstruction_index` | Chosen break-down index, or `null` (also `null` for a single break-down — trivially selected). |
| `deconstruction_locked` | Whether the break-down filter is locked. |
| `component_selected_uids{}` | `component word → chosen result uid` (deconstructor-resolved words only). |

**Rendering case partition (FR-A5)** — how a word resolved decides its UI and AI
items: (a) one result → static; (b) ≥ 2 **direct** senses → one sense choice;
(c)/(d) `direct_uids` empty + break-downs → per-component senses, with a
break-down selector when ≥ 2 break-downs. **Mixed words** (both direct results
and deconstructions, e.g. *sādhūti*) render as (a)/(b) — the direct match
resolves them; their `deconstructions` are still populated for consumers.

```sh
PORT=$(cat "$SIMSAPA_DIR/api-port.txt" 2>/dev/null || echo 4848)

# A deconstructor-resolved compound (no direct match): atthaññe = atthi + aññe.
curl -s -X POST "localhost:$PORT/gloss_text" \
  -H 'Content-Type: application/json' -d '{"text":"atthaññe"}' \
  | jq '.paragraphs[0].words_data[0]
        | {stem, direct_uids, selected_deconstruction_index,
           deconstructions: (.deconstructions
             | map({words_joined, components: (.components|map({word, result_uids}))}))}'
```

```jsonc
{
  "stem": "atthi 1.1",
  "direct_uids": [],                       // empty ⇒ deconstructor-resolved
  "selected_deconstruction_index": null,   // single break-down ⇒ trivially selected
  "deconstructions": [
    { "words_joined": "atthi + aññe",
      "components": [
        { "word": "atthi", "result_uids": ["2736/dpd","2737/dpd","2738/dpd","2757/dpd"] },
        { "word": "aññe",  "result_uids": ["1497/dpd","1498/dpd","1601/dpd","1631/dpd","1660/dpd","74340/dpd","80675/dpd"] }
      ] }
  ]
}
```

Multi-break-down compound (case (d)) — *pañcaggadāyakaṁ* returns four break-downs
(`pañca + agga + dāyakaṁ`, `pañca + gadā + yakaṁ`, …) with `direct_uids: []`; a
mixed word like *sādhūti* returns non-empty `direct_uids` **and** two break-downs
(`sādhu + iti`, `sādhū + iti`).

### 16.2 `GET /word_selection_ws` (WebSocket)

Runs the AI Word Selection engine over a glossed result and streams progress.
**Uses the app's configured provider/model fallback lists and stored API keys**
(the route takes **no** per-request key — configure one first with
`POST /set_ai_provider_key`, §16.4), the in-app pacing constants (char limit
40000, ≥ 6.5 s between batches, 180 s timeout) and the same retry schedule (rounds at 10/20/30/40/50 s,
provider-skip on `auth`/`quota_exceeded`, `invalid_response` retry on truncated
bodies). Error classification reuses the `ai_error` envelope. The engine runs on
a dedicated thread; the handler is a pure protocol adapter.

**Client → server messages** (JSON text frames, `type`-tagged):

| Message | Meaning |
|---------|---------|
| `{"type":"request","paragraphs":[{"paragraph_index":0,"words_json":"<serialized words_data>"},…]}` | Start one run. `words_json` is a paragraph's `words_data` array serialized as a **string** (i.e. the JSON stringified). Item ids embed `paragraph_index`. |
| `{"type":"cancel"}` | Abandon the in-flight run. |

**Server → client messages:**

| Message | Meaning |
|---------|---------|
| `{"type":"status","stage":"model_attempt"\|"retry_wait"\|"attempt_failed","model":…,"message":…,"batch":n,"batch_count":m}` | Progress. `batch`/`batch_count` track server-side batching (one batch per paragraph when the combined prompt exceeds the char limit; otherwise a single batch). |
| `{"type":"error","message":"…"}` | Protocol error (invalid message; a second `request` while one is running). |
| `{"type":"error","ai_error":{…},"batch":n}` | A batch's AI failure after the walk exhausted retries — the classified `ai_error` envelope; the run continues with later batches. |
| `{"type":"result","selections":[…]}` | **Terminal.** Lenient-validated selections; the server closes the socket after sending it. |

Each `selections[]` entry is `{ "id": "p<pi>w<wi>[d|c<k>]", "uid": "…", "confidence": "…" }`:

- `p<pi>w<wi>` — a **sense** answer (`uid` = chosen result uid).
- `p<pi>w<wi>d` — a **break-down** answer for a ≥ 2-break-down compound; `uid` is
  a pseudo-uid `d:<n>` selecting `selected_deconstruction_index = n` (and locks it).
- `p<pi>w<wi>c<k>` — a **component-sense** answer; `uid` is that component's chosen
  result uid, written to `component_selected_uids`.

**Protocol notes:**

- **One run per connection.** A second `request` while one is active gets a
  typed `error` ("already in progress"), not a second walk.
- **Cancellation parity, not improvement.** The walk checks the cancel flag
  between attempts and once per second during backoff sleeps; an **in-flight HTTP
  attempt is not aborted** (same as in-app). So after `cancel` the server may
  emit a few more `status` messages until the current attempt resolves, then
  closes. A socket close or a failed send also counts as cancel.
- **Empty ask.** If nothing is ambiguous (no `d`/`c`/sense items to resolve), the
  server sends `{"type":"result","selections":[]}` immediately.

**Live transcript** (driven with a minimal stdlib WebSocket client; the app was
running with Gemini configured). Request with a compound `pañcaggadāyakaṁ` and a
following word `dhammā`:

```jsonc
// → {"type":"request","paragraphs":[{"paragraph_index":0,"words_json":"<words_data string>"}]}
// ← status:
{"type":"status","stage":"model_attempt","model":"gemini-3-flash-preview",
 "message":"Request sent to Gemini (gemini-3-flash-preview)…","batch":0,"batch_count":1}
// ← result (terminal): the d item picked break-down 2, plus each ambiguous component:
{"type":"result","selections":[
  {"id":"p0w0d","uid":"d:2","confidence":"confident"},         // break-down index 2
  {"id":"p0w0c1","uid":"24236/dpd","confidence":"confident"},  // component sense
  {"id":"p0w0c2","uid":"20863/dpd","confidence":"confident"},
  {"id":"p0w0c3","uid":"24236/dpd","confidence":"confident"},
  {"id":"p0w0c5","uid":"562/dpd","confidence":"confident"},
  {"id":"p0w0c6","uid":"32309/dpd","confidence":"confident"},
  {"id":"p0w1","uid":"34630/dpd","confidence":"confident"}      // dhammā sense
]}
```

Error / robustness paths (from the edge-case client):

```jsonc
// Non-JSON or unknown message:
// → this is not json
// ← {"type":"error","message":"Invalid message: expected ident at line 1 column 2"}

// A second request while one runs:
// → {"type":"request",…}   (starts; ← status model_attempt)
// → {"type":"request",…}   (rejected)
// ← {"type":"error","message":"A word-selection run is already in progress"}
// … then the first run's ← {"type":"result",…} still arrives.
```

### 16.3 Agent quick-start: gloss → word-selection table

A client renders a vocabulary table in two steps:

1. **`POST /gloss_text`** with the passage → for each `words_data[]` word show a
   row. Cases (a)/(b): the word + its senses (`results` filtered by
   `selected_index`). Cases (c)/(d) (`direct_uids` empty): one **indented
   sub-row per component** (`deconstructions[selected_deconstruction_index]
   .components`, or the union across break-downs when unlocked), each with its
   `result_uids`.
2. **`GET /word_selection_ws`** — send one `request` carrying each paragraph's
   `words_data` **stringified** as `words_json`; show `status` messages as
   progress ("trying model X…", "retry in N s…"), surface `error` messages
   (`ai_error.message` for AI failures), and on the terminal `result` apply each
   selection by `id`: `p…w…` → set the word's sense, `…d` → set the break-down
   (`d:<n>`), `…c<k>` → set a component's sense (`uid` into
   `component_selected_uids`).

Both routes are localhost-only and unauthenticated (like the rest of the API).
Fetch full word entries for any `uid` via §13 (`/words/<uid>.json`,
`/get_word_html_by_uid/…`).

### 16.4 `POST /set_ai_provider_key` + a self-contained demo page

The `/word_selection_ws` engine reads the app's **saved** provider settings —
there is no per-request key. So before a client can run AI selection it must
configure a working provider. `POST /set_ai_provider_key` is a small
dev/demo convenience route that does exactly that.

- **Request:** `{ "provider": "Gemini", "api_key": "AIza…", "model"?: "…" }`.
  `provider` is a canonical provider name (`"Gemini"`, `"OpenRouter"`,
  `"Anthropic"`, `"OpenAI"`, `"DeepSeek"`, `"xAI"`, `"Mistral"`, `"HuggingFace"`,
  `"Perplexity"`, `"NvidiaNim"`, `"SambaNova"`). `model` is optional — an id to
  enable (added if the app doesn't know it).
- **Behavior:** sets the provider's key and **enables** it (which syncs its
  enabled models into the fallback sequence), then **moves that provider's
  entries to the front** of the fallback sequence so it is tried first (rather
  than failing auth on other enabled providers). An **empty `api_key` clears the
  key and disables the provider**.
- **Response** (never echoes the key): `{ provider, enabled, has_key,
  fallback_sequence: [{ provider, model_name, enabled }, …] }`.
- **Errors:** 400 on an unparseable body.

```sh
# Configure Gemini for the WebSocket engine (uses the app's stored settings).
curl -s -X POST "localhost:$PORT/set_ai_provider_key" \
  -H 'Content-Type: application/json' \
  -d '{"provider":"Gemini","api_key":"AIza…"}' | jq '{enabled, has_key, models:(.fallback_sequence|length)}'
```

**`GET /ai_provider_key?provider=Gemini`** reads a provider's currently-saved
key + enabled state so a localhost client can **prefill** its key field. It
returns `{ provider, enabled, has_key, api_key }` — `api_key` is the stored
value (the demo page fetches this on load; `provider` defaults to `Gemini`).

```sh
curl -s "localhost:$PORT/ai_provider_key?provider=Gemini" | jq '{enabled, has_key}'
```

> This route persists into the app's real AI settings (it is the same store the
> in-app **Models** dialog edits). It is meant for a trusted localhost demo/dev
> client — the demo below sends the key here so the WebSocket run can reach
> Gemini.

**The demo page.** [`gloss_demo.html`](https://github.com/simsapa/simsapa/blob/main/scripts/gloss_demo.html), in the Simsapa repository, is a
single self-contained HTML file (no build, no dependencies) that ties the three
routes together: a textarea for a few paragraphs, a **Gloss** button
(`POST /gloss_text` → a vocabulary list like the in-app Gloss tab), a **Gemini
API key** field (**prefilled on load** from the app's saved settings via
`GET /ai_provider_key` when one exists), and an **AI word selection** checkbox
(checked by default,
**disabled while the key field is empty**). With a key present and the box
checked, after glossing it POSTs the key to `/set_ai_provider_key`, opens
`/word_selection_ws`, streams the status/error messages into a log panel, and
updates the vocabulary table (sense dropdowns, compound break-down + component
sub-rows) when the final `result` applies. Open the file in a browser and point
its **API port** field at your running instance.

The whole page, for reference (identical to the committed file):

```html
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Simsapa Gloss Demo</title>
<style>
  :root { --border: #d9d4c8; --bg: #faf8f2; --card: #fff; --ink: #2b2b2b;
          --muted: #6b6b6b; --accent: #7a5c1e; --field-bg: #fff; }
  @media (prefers-color-scheme: dark) {
    :root { --border: #3a3a3a; --bg: #1b1b1b; --card: #242424; --ink: #e6e3dc;
            --muted: #9a948a; --accent: #c9a54e; --field-bg: #2b2b2b; }
  }
  * { box-sizing: border-box; }
  body { font-family: system-ui, -apple-system, "Segoe UI", sans-serif;
         margin: 0; background: var(--bg); color: var(--ink); line-height: 1.4; }
  .wrap { max-width: 880px; margin: 0 auto; padding: 1.2rem; }
  h1 { font-size: 1.3rem; margin: 0 0 .2rem; }
  p.lede { color: var(--muted); margin: 0 0 1rem; font-size: .9rem; }
  .card { background: var(--card); border: 1px solid var(--border);
          border-radius: 8px; padding: 1rem; margin-bottom: 1rem; }
  label { font-size: .85rem; font-weight: 600; display: block; margin: .1rem 0 .3rem; }
  textarea { width: 100%; min-height: 6rem; font: inherit; padding: .6rem;
             border: 1px solid var(--border); border-radius: 6px; resize: vertical;
             background: var(--field-bg); color: var(--ink); }
  input[type=text], input[type=password], input.port {
    font: inherit; padding: .45rem .6rem; border: 1px solid var(--border);
    border-radius: 6px; background: var(--field-bg); color: var(--ink); }
  input[type=password] { width: 100%; }
  input.port { width: 6rem; }
  .row { display: flex; gap: 1rem; align-items: flex-end; flex-wrap: wrap; margin-top: .8rem; }
  .row > div { flex: 1 1 auto; }
  .checkline { display: flex; align-items: center; gap: .4rem; font-size: .9rem; }
  .checkline label { margin: 0; font-weight: 400; }
  button { font: inherit; font-weight: 600; padding: .55rem 1.1rem; cursor: pointer;
           background: var(--accent); color: #fff; border: none; border-radius: 6px; }
  button:disabled { opacity: .5; cursor: default; }
  .hint { font-size: .78rem; color: var(--muted); margin-top: .3rem; }
  #log { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: .78rem;
         background: #1e1e1e; color: #dcdcdc; border-radius: 6px; padding: .6rem .7rem;
         max-height: 12rem; overflow: auto; white-space: pre-wrap; display: none; }
  #log.show { display: block; }
  .log-status { color: #6fb3ff; }
  .log-error  { color: #ff8a80; }
  .log-result { color: #b5f4a5; }
  .word { padding: .5rem .2rem; border-bottom: 1px solid var(--border); }
  .word:last-child { border-bottom: none; }
  .headword { font-weight: 700; font-size: 1rem; }
  .headword .stem { color: var(--muted); font-weight: 400; font-size: .82rem; margin-left: .4rem; }
  .sense { margin-top: .25rem; }
  select { font: inherit; max-width: 100%; padding: .25rem; border: 1px solid var(--border);
           border-radius: 5px; background: var(--field-bg); color: var(--ink); }
  .summary { font-size: .82rem; color: var(--ink); margin-top: .25rem; }
  .summary i { color: var(--muted); }
  .components { margin-top: .4rem; }
  .component { margin: .3rem 0 .3rem 20px; padding-left: .6rem; border-left: 2px solid var(--border); }
  .component .cword { font-weight: 600; font-size: .9rem; }
  .breakdown { margin-top: .3rem; display: flex; align-items: center; gap: .5rem; flex-wrap: wrap; }
  .para-title { font-size: .8rem; text-transform: uppercase; letter-spacing: .04em;
                color: var(--muted); margin: .2rem 0 .3rem; }
  .unrec { font-size: .8rem; color: var(--muted); margin-top: .5rem; }
  .empty { color: var(--muted); font-size: .9rem; }
</style>
</head>
<body>
<div class="wrap">
  <h1>Simsapa Gloss Demo</h1>
  <p class="lede">Glosses Pāli text via <code>POST /gloss_text</code> and (optionally) runs AI
     word&nbsp;selection over the <code>GET /word_selection_ws</code> WebSocket. Point it at a
     running Simsapa instance.</p>

  <div class="card">
    <label for="text">Pāli text (blank line separates paragraphs)</label>
    <textarea id="text">Sādhu bhante, pañcaggadāyakaṁ dhammaṁ desetu.

Atthaññe kusalā dhammā.</textarea>

    <div class="row">
      <div style="flex: 0 0 auto;">
        <label for="port">API port</label>
        <input class="port" id="port" type="text" value="4848" inputmode="numeric">
      </div>
      <div>
        <label for="key">Gemini API key (configures the app for AI selection)</label>
        <input id="key" type="password" placeholder="AIza… — leave empty to gloss only" autocomplete="off">
      </div>
    </div>

    <div class="row">
      <div class="checkline" style="flex: 0 0 auto;">
        <input type="checkbox" id="ai" checked disabled>
        <label for="ai">AI word selection</label>
      </div>
      <div style="flex: 0 0 auto;">
        <button id="go">Gloss</button>
      </div>
    </div>
    <div class="hint">The WebSocket uses the app's saved provider keys, so the key is first sent to
       <code>POST /set_ai_provider_key</code> (Gemini, enabled &amp; prioritized). The key is never
       stored by this page.</div>
  </div>

  <div id="log"></div>
  <div id="out" class="card"><span class="empty">Enter text and press <b>Gloss</b>.</span></div>
</div>

<script>
"use strict";

const $ = (id) => document.getElementById(id);
const base = () => `http://localhost:${($("port").value || "4848").trim()}`;
const wsBase = () => `ws://localhost:${($("port").value || "4848").trim()}`;

// --- key field gates the AI checkbox -----------------------------------------
// Checked by default; disabled (greyed, and so inactive) while the key is empty.
// `aiEnabled()` requires the box to be both checked AND enabled, so a
// checked-but-disabled box never runs AI.
function syncAiCheckbox() {
  $("ai").disabled = $("key").value.trim().length === 0;
}
$("key").addEventListener("input", syncAiCheckbox);
syncAiCheckbox();

const aiEnabled = () => $("ai").checked && !$("ai").disabled;

// On load, prefill the key from the app's saved settings if one exists.
async function prefillKey() {
  try {
    const r = await fetch(base() + "/ai_provider_key?provider=Gemini");
    if (!r.ok) return;
    const j = await r.json();
    if (j.api_key && !$("key").value) { $("key").value = j.api_key; syncAiCheckbox(); }
  } catch (_) { /* app not running yet; user can type a key */ }
}
prefillKey();

// --- small helpers -----------------------------------------------------------
function stripHtml(s) {
  const d = document.createElement("div"); d.innerHTML = s || ""; return d.textContent || "";
}
function truncate(s, n) { s = s || ""; return s.length > n ? s.slice(0, n - 1) + "…" : s; }

function log(kind, msg) {
  const box = $("log"); box.classList.add("show");
  const line = document.createElement("div");
  line.className = "log-" + kind;
  line.textContent = msg;
  box.appendChild(line); box.scrollTop = box.scrollHeight;
}
function clearLog() { const b = $("log"); b.textContent = ""; b.classList.remove("show"); }

// A word is "deconstructor-resolved" when it has no direct match but has break-downs.
const isCompound = (w) => (w.direct_uids || []).length === 0 && (w.deconstructions || []).length > 0;
// Sense options of a direct/mixed word = its results limited to the direct uids.
function senseResults(w) {
  const direct = new Set(w.direct_uids || []);
  const r = (w.results || []).filter((x) => direct.has(x.uid));
  return r.length ? r : (w.results || []);   // fallback: show all if direct set is unexpectedly empty
}
// uid -> result record, for looking up a component's option summaries.
function resultIndex(w) {
  const m = new Map();
  for (const r of (w.results || [])) m.set(r.uid, r);
  return m;
}
// Dedup, first-appearance order of ALL component words across all break-downs.
// The AI `c<k>` id is the 0-based position in THIS list.
function componentEnumeration(w) {
  const seen = new Set(), list = [];
  for (const d of (w.deconstructions || []))
    for (const c of (d.components || []))
      if (!seen.has(c.word)) { seen.add(c.word); list.push(c.word); }
  return list;
}
// The components to show: locked+selected -> that break-down's; else the union.
function visibleComponents(w) {
  const locked = w.deconstruction_locked && w.selected_deconstruction_index != null;
  if (locked) return (w.deconstructions[w.selected_deconstruction_index].components) || [];
  const seen = new Set(), list = [];
  for (const d of (w.deconstructions || []))
    for (const c of (d.components || []))
      if (!seen.has(c.word)) { seen.add(c.word); list.push(c); }
  return list;
}

// --- rendering ---------------------------------------------------------------
let DATA = null;   // last gloss result; mutated in place as selections apply

function render() {
  const out = $("out");
  out.innerHTML = "";
  if (!DATA || !DATA.paragraphs || !DATA.paragraphs.length) {
    out.innerHTML = '<span class="empty">No words.</span>'; return;
  }
  for (const para of DATA.paragraphs) {
    if (DATA.paragraphs.length > 1) {
      const t = document.createElement("div");
      t.className = "para-title"; t.textContent = "Paragraph " + (para.paragraph_index + 1);
      out.appendChild(t);
    }
    para.words_data.forEach((w, wi) => out.appendChild(renderWord(para.paragraph_index, wi, w)));
    if ((para.unrecognized_words || []).length) {
      const u = document.createElement("div");
      u.className = "unrec"; u.textContent = "Unrecognized: " + para.unrecognized_words.join(", ");
      out.appendChild(u);
    }
  }
}

function renderWord(pi, wi, w) {
  const el = document.createElement("div");
  el.className = "word";
  const head = document.createElement("div");
  head.className = "headword";
  head.innerHTML = (w.original_word || w.stem || "?") +
    (w.stem && w.stem !== w.original_word ? '<span class="stem">' + w.stem + "</span>" : "");
  el.appendChild(head);

  if (isCompound(w)) {
    renderCompound(el, pi, wi, w);
  } else {
    renderDirect(el, pi, wi, w);
  }
  return el;
}

// (a)/(b): single sense = static; multiple = a <select>.
function renderDirect(el, pi, wi, w) {
  const opts = senseResults(w);
  const chosen = w._sense_uid || (opts[w.selected_index] && opts[w.selected_index].uid) ||
                 (opts[0] && opts[0].uid);
  if (opts.length <= 1) {
    const s = document.createElement("div"); s.className = "summary";
    s.innerHTML = opts[0] ? opts[0].summary : "<i>(no entry)</i>";
    el.appendChild(s); return;
  }
  const sel = document.createElement("select");
  opts.forEach((o) => {
    const op = document.createElement("option");
    op.value = o.uid; op.textContent = truncate(stripHtml(o.summary) || o.uid, 90);
    if (o.uid === chosen) op.selected = true;
    sel.appendChild(op);
  });
  sel.value = chosen;
  const detail = document.createElement("div"); detail.className = "summary";
  const setDetail = (uid) => {
    const o = opts.find((x) => x.uid === uid);
    detail.innerHTML = o ? o.summary : "";
  };
  sel.addEventListener("change", () => { w._sense_uid = sel.value; setDetail(sel.value); });
  const line = document.createElement("div"); line.className = "sense";
  line.appendChild(sel);
  el.appendChild(line); el.appendChild(detail);
  setDetail(chosen);
}

// (c)/(d): component sub-rows, plus a break-down selector for >= 2 break-downs.
function renderCompound(el, pi, wi, w) {
  const idx = resultIndex(w);

  if ((w.deconstructions || []).length > 1) {
    const bd = document.createElement("div"); bd.className = "breakdown";
    const sel = document.createElement("select");
    w.deconstructions.forEach((d, i) => {
      const op = document.createElement("option");
      op.value = String(i); op.textContent = d.words_joined;
      sel.appendChild(op);
    });
    // Set the current value explicitly — option.selected during construction is
    // not honored reliably, which left the dropdown on break-down 0 after an AI pick.
    if (w.selected_deconstruction_index != null)
      sel.value = String(w.selected_deconstruction_index);
    const lock = document.createElement("input"); lock.type = "checkbox";
    lock.checked = !!w.deconstruction_locked;
    const lockLbl = document.createElement("label");
    lockLbl.style.fontWeight = "400"; lockLbl.style.fontSize = ".85rem"; lockLbl.style.margin = "0";
    lockLbl.textContent = "lock"; lockLbl.style.display = "inline";
    sel.addEventListener("change", () => {
      w.selected_deconstruction_index = parseInt(sel.value, 10);
      w.deconstruction_locked = true; lock.checked = true; render();
    });
    lock.addEventListener("change", () => {
      w.deconstruction_locked = lock.checked;
      if (lock.checked && w.selected_deconstruction_index == null)
        w.selected_deconstruction_index = parseInt(sel.value, 10);
      render();
    });
    bd.appendChild(document.createTextNode("break-down: "));
    bd.appendChild(sel); bd.appendChild(lock); bd.appendChild(lockLbl);
    el.appendChild(bd);
  }

  const box = document.createElement("div"); box.className = "components";
  for (const comp of visibleComponents(w)) {
    box.appendChild(renderComponent(w, comp, idx));
  }
  el.appendChild(box);
}

function renderComponent(w, comp, idx) {
  const el = document.createElement("div"); el.className = "component";
  const name = document.createElement("span"); name.className = "cword"; name.textContent = comp.word;
  el.appendChild(name);

  const uids = comp.result_uids || [];
  w.component_selected_uids = w.component_selected_uids || {};
  const chosen = w.component_selected_uids[comp.word] || uids[0];

  if (uids.length <= 1) {
    const s = document.createElement("div"); s.className = "summary";
    const r = idx.get(uids[0]);
    s.innerHTML = r ? r.summary : "<i>(no entry)</i>";
    el.appendChild(s); return el;
  }
  const sel = document.createElement("select");
  uids.forEach((uid) => {
    const r = idx.get(uid);
    const op = document.createElement("option");
    op.value = uid; op.textContent = truncate(stripHtml(r ? r.summary : "") || uid, 80);
    if (uid === chosen) op.selected = true;
    sel.appendChild(op);
  });
  sel.value = chosen;
  const detail = document.createElement("div"); detail.className = "summary";
  const setDetail = (uid) => { const r = idx.get(uid); detail.innerHTML = r ? r.summary : ""; };
  sel.addEventListener("change", () => { w.component_selected_uids[comp.word] = sel.value; setDetail(sel.value); });
  const line = document.createElement("span"); line.style.marginLeft = ".4rem";
  line.appendChild(sel);
  el.appendChild(line);
  el.appendChild(detail); setDetail(chosen);
  return el;
}

// --- applying WebSocket selections -------------------------------------------
function wordAt(pi, wi) {
  const para = DATA.paragraphs.find((p) => p.paragraph_index === pi);
  return para ? para.words_data[wi] : null;
}

function applySelection(entry) {
  const m = /^p(\d+)w(\d+)(?:(d)|c(\d+))?$/.exec(entry.id || "");
  if (!m) return;
  const pi = +m[1], wi = +m[2], w = wordAt(pi, wi);
  if (!w) return;

  if (m[3] === "d") {                                   // break-down: uid = "d:<n>"
    const n = parseInt(String(entry.uid).replace(/^d:/, ""), 10);
    if (!Number.isNaN(n)) { w.selected_deconstruction_index = n; w.deconstruction_locked = true; }
  } else if (m[4] != null) {                            // component sense: c<k>
    const k = +m[4], compWord = componentEnumeration(w)[k];
    if (compWord) {
      w.component_selected_uids = w.component_selected_uids || {};
      w.component_selected_uids[compWord] = entry.uid;
    }
  } else {                                              // direct sense
    w._sense_uid = entry.uid;
  }
}

function runWordSelection(key) {
  return new Promise(async (resolve) => {
    // 1. Configure the app so the WS engine can reach Gemini.
    try {
      const r = await fetch(base() + "/set_ai_provider_key", {
        method: "POST", headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ provider: "Gemini", api_key: key }),
      });
      const j = await r.json();
      log("status", `Configured Gemini (enabled=${j.enabled}, fallback has ${j.fallback_sequence.length} model(s)).`);
    } catch (e) { log("error", "set_ai_provider_key failed: " + e); resolve(); return; }

    // 2. Open the WebSocket and stream the run.
    const paragraphs = DATA.paragraphs.map((p) => ({
      paragraph_index: p.paragraph_index, words_json: JSON.stringify(p.words_data),
    }));
    let ws;
    try { ws = new WebSocket(wsBase() + "/word_selection_ws"); }
    catch (e) { log("error", "WebSocket failed: " + e); resolve(); return; }

    ws.onopen = () => {
      log("status", "WebSocket open — requesting word selection…");
      ws.send(JSON.stringify({ type: "request", paragraphs }));
    };
    ws.onmessage = (ev) => {
      let msg; try { msg = JSON.parse(ev.data); } catch { log("error", "bad frame: " + ev.data); return; }
      if (msg.type === "status") {
        const b = (msg.batch_count > 1) ? ` [batch ${msg.batch + 1}/${msg.batch_count}]` : "";
        log("status", `${msg.stage}: ${msg.message || msg.model || ""}${b}`);
      } else if (msg.type === "error") {
        const detail = msg.ai_error ? (msg.ai_error.message || JSON.stringify(msg.ai_error)) : msg.message;
        log("error", "error: " + detail);
      } else if (msg.type === "result") {
        (msg.selections || []).forEach(applySelection);
        log("result", `Applied ${(msg.selections || []).length} selection(s).`);
        render();
        ws.close();
      }
    };
    ws.onerror = () => log("error", "WebSocket error.");
    ws.onclose = () => { log("status", "WebSocket closed."); resolve(); };
  });
}

// --- main action -------------------------------------------------------------
async function gloss() {
  const text = $("text").value;
  const wantAi = aiEnabled();
  const key = $("key").value.trim();
  $("go").disabled = true; clearLog();
  $("out").innerHTML = '<span class="empty">Glossing…</span>';
  try {
    const r = await fetch(base() + "/gloss_text", {
      method: "POST", headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ text }),
    });
    if (!r.ok) throw new Error("HTTP " + r.status);
    DATA = await r.json();
    render();
    if (wantAi && key) {
      log("status", "Starting AI word selection…");
      await runWordSelection(key);
    }
  } catch (e) {
    $("out").innerHTML = '<span class="empty">Error: ' + stripHtml(String(e)) + "</span>";
  } finally {
    $("go").disabled = false;
  }
}
$("go").addEventListener("click", gloss);
</script>
</body>
</html>
```

### 16.5 Agent-driven word selection (no AI provider)

With `simsapa serve`, or with `simsapa gloss --json` (which prints the same
document as `/gloss_text`), there is no `/word_selection_ws`: the calling agent
is the language model, and it makes the choices the WebSocket would make. No
provider key and no WebSocket are involved. Everything it needs is in the
`/gloss_text` response:

1. For each `paragraphs[].words_data[]` word, **`direct_uids` decides what to
   choose.** Non-empty: choose one sense among `direct_uids` (cases (a)/(b) of
   §16.1). Empty: the word is deconstructor-resolved; choose one of
   `deconstructions[]` (when there is more than one), then one sense per
   component of that break-down, from the component's `result_uids` (cases
   (c)/(d)).
2. **The meanings are in `results[]`**, keyed by `uid`: each has `word` (the
   headword, e.g. `dhamma 1.03`) and `summary` (part of speech, meaning,
   construction, as HTML). `direct_uids` and `result_uids` are uids into that
   list. A uid not in it (e.g. one dropped by `no_duplicates_globally`) can be
   fetched with `/word.json?uid=` (§13.3).
3. **The context** is the passage itself; `example_sentence` carries the
   ±50-character window around the word.
4. The chosen `uid`s are ordinary word uids: render or export them with §13.
   The numbers in `selected_index` and `selected_deconstruction_index` are the
   pipeline's defaults (the first option), not a judgement.

A jq program that reduces the response to the choices, with each option's
meaning as plain text:

```sh
cat > choices.jq <<'EOF'
.paragraphs[].words_data[]
| (.results | map({key: .uid, value: (.word + ": " + (.summary | gsub("<[^>]*>"; "")))}) | from_entries) as $sense
| if (.direct_uids | length) > 0 then
    { word: .original_word, choose: "sense",
      options: [.direct_uids[] | $sense[.]] }
  else
    { word: .original_word, choose: "break-down, then a sense per component",
      breakdowns: [.deconstructions | to_entries[] |
        { index: .key, words_joined: .value.words_joined,
          components: [.value.components[] |
            { word, options: [.result_uids[] | $sense[.]] }] }] }
  end
EOF

curl -s -X POST "localhost:$PORT/gloss_text" -H 'Content-Type: application/json' \
  -d '{"text":"pañcaggadāyakaṁ bhikkhave"}' | jq -f choices.jq
# or, without a server:
simsapa gloss "pañcaggadāyakaṁ bhikkhave" --json | jq -f choices.jq
```

Output (shortened):

```jsonc
{
  "word": "pañcaggadāyakaṁ",
  "choose": "break-down, then a sense per component",
  "breakdowns": [
    { "index": 0, "words_joined": "pañca + gadāya + kaṁ", "components": [ … ] },
    { "index": 1, "words_joined": "pañca + gadā + yakaṁ", "components": [ … ] },
    { "index": 2, "words_joined": "pañca + agga + dāyakaṁ",
      "components": [
        { "word": "pañca",   "options": ["pañca: (card) five (5)   card, x pl"] },
        { "word": "agga",    "options": ["agga 1.1: (adj) highest; topmost; foremost; supreme; ultimate   adj",
                                         "agga 1.3: (nt) tip; top; peak   nt", "…"] },
        { "word": "dāyakaṁ", "options": ["dāyaka 1: (masc) giver; donor; sponsor (of)  [√dā + [y] + aka]  masc, agent, from dadāti",
                                         "dāyaka 2: (adj) giving; offering; providing  [√dā + [y] + aka]  adj, in comps, from dadāti"] }
      ] },
    { "index": 3, "words_joined": "pañca + agga + adāyakaṁ", "components": [ … ] }
  ]
}
{
  "word": "bhikkhave",
  "choose": "sense",
  "options": [
    "bhikkhave: (masc) monks  [√bhikkh + u + ave], [bhikkhu + ave]  masc, voc pl of bhikkhu",
    "bhikkhu: (masc) monk; monastic; mendicant; fully ordained monk  [√bhikkh + u]  masc, from bhikkhati"
  ]
}
```

Here the agent picks break-down 2 (*pañca + agga + dāyakaṁ*, "giver of the five
first-fruits"), `agga 1.4` for *agga* and `dāyaka 1` for *dāyakaṁ*, and the
vocative `bhikkhave` for the second word. Break-downs 0 and 1 are the
deconstructor's mechanical splits; judging them is exactly the agent's job.

To check a form exactly rather than gloss a passage, `/lookup_forms` (or
`simsapa lookup`) returns every headword and split with nothing chosen (§19.1).

## 17. Reference conversion: `POST /reference_search`

The search of the app's **Reference Search** window, for
converting a sutta reference from one style to another — typically a PTS volume
and page (`M iii 24`) to the SuttaCentral reference (`MN 110`).

It reads a reference table embedded in the program, so it needs no index and
works before any search has run. It is the same search the window runs.

This is a different mechanism from the reference auto-detect of the sutta search
routes (§5), which recognises SuttaCentral-style references only and returns
sutta records. A PTS volume and page means nothing to those routes.

### 17.1 Request

```json
{ "query_text": "M iii 24", "field": "pts_reference", "limit": 100 }
```

| Field | Meaning |
|---|---|
| `query_text` | The reference or title to look up. Required. |
| `field` | Which column to search — the window's drop-down. Optional, default `pts_reference`. An unrecognized value is **HTTP 400** with the accepted names in the body. |
| `limit` | The most rows to return. Optional, default 100; `0` returns every match. |

| `field` | Matching |
|---|---|
| `pts_reference` | **Page-range match.** The query is parsed into book, volume and page, and matches the sutta whose PTS pages contain that page — `M iii 24` finds the sutta printed on M iii 20–25. |
| `sutta_ref` | Substring of the SuttaCentral reference (`MN 110`). |
| `title_pali` | Substring of the Pāli title (`Cūḷapuṇṇamasutta`). |
| `dpr_reference` | Substring of the Digital Pāli Reader reference (`KN 2.1`). |

The three substring fields compare lowercased and without diacritics, so
`satipatthana` finds `Satipaṭṭhānasutta`.

### 17.2 Response

```json
{
  "hits": 1,
  "results": [
    {
      "uid": "mn110/pli/ms",
      "sc_uid": "mn110",
      "sutta_ref": "MN 110",
      "title_pali": "Cūḷapuṇṇamasutta",
      "pts_reference": "M iii 20",
      "dpr_reference": null,
      "dpr_reference_alt": null,
      "url": "https://suttacentral.net/mn110",
      "pts_nikaya": "M",
      "pts_vol": "iii",
      "pts_vol_verse": null,
      "pts_start_page": 20,
      "pts_end_page": 25,
      "edition": null
    }
  ]
}
```

`hits` counts every match; `results` holds at most `limit` of them, so
`hits > results.length` means the answer was cut. No match is HTTP 200 with
`hits: 0`. There is no paging: narrow the query, or raise `limit`.

| Field | Meaning |
|---|---|
| `uid` | The uid of the text **in this app's database**, ready for the retrieval routes (§13): the Pāli Mahāsaṅgīti text when there is one, else another edition of the same uid. **`null` when the row has no directly matching uid** — see §17.3. |
| `sc_uid` | SuttaCentral's id for the row, the last path segment of `url`. This is what "SuttaCentral style" means as an identifier. **Not a uid for this API** — see §17.3. |
| `sutta_ref` | The SuttaCentral-style reference as cited (`MN 110`). |
| `title_pali` | The Pāli title; may be empty. |
| `pts_reference` | The PTS reference of the sutta's **first** page; empty on the rows the table has no PTS reference for. |
| `pts_nikaya`, `pts_vol`, `pts_start_page`, `pts_end_page` | The parsed PTS reference the range match runs on. `pts_end_page` is the page the sutta ends on. |
| `pts_vol_verse` | The verse span, for verse collections (`Dh v1-20`). |
| `dpr_reference`, `dpr_reference_alt` | Digital Pāli Reader references; null on most rows. |
| `edition` | The PTS edition where the table distinguishes two (`Feer`, `Somaratne1999`), else null. |
| `url` | The SuttaCentral page. |

### 17.3 `uid` and `sc_uid` are not interchangeable

The table is SuttaCentral's, and it is wider than this app's database. Take the
text from `uid`, and the citation from `sutta_ref` / `sc_uid`:

- **`uid` is `null` for texts the app does not hold** — the Chinese Āgama
  parallels (`sa…`, `ma…`, `ea…`), among others.
- **`uid` is also `null` where the app stores a text under a wider uid** than
  SuttaCentral's id: a Dhammapada chapter is `dhp21-32` here and `dhp2` there.
  `null` therefore does not prove the text is absent.
- **Never pass `sc_uid` to a sutta route in place of a `null` `uid`.** The sutta
  routes read `dhp2` as *verse* 2 and return chapter 1 (§14.4), while the table
  means *chapter* 2. Search for the title or the verse number instead.

### 17.4 Reading the result of a PTS lookup

- **Several rows are a real answer, not noise.** A PTS page often holds more
  than one short sutta: `A i 10` returns AN 1.47 – AN 1.54. Rows whose sutta
  *starts* on the queried page come first, then those that merely span it.
  Which of them a citation means cannot be decided from the page alone.
- **One sutta can appear once per edition.** `S i 1` returns SN 1.1 three times:
  `edition` null, `Feer` and `Somaratne1999`. Their pages can differ, so a
  citation of the Saṁyutta's first volume needs its edition to be converted
  exactly.
- **Accepted spellings of one reference:** `M iii 24`, `m iii 24`, `M.III.24`,
  `MN iii 24`, `PTS M iii 24`, `M iii 24-25`, `M iii 24–25`, `M iii 24ff`. The
  book is the PTS abbreviation (`A`, `D`, `M`, `S`, `Dh`, `It`, `Kh`, `Sn`, `Th`,
  `Ud`, `Pv`, `Vv`, `Ap`, `Cv`, `Nett`, `Pet`, `Vin`); `DN`, `MN`, `SN` and
  `AN` are read as `D`, `M`, `S` and `A` **when a volume follows**.
- **Without a volume, `Sn` is the Suttanipāta**, never the Saṁyutta. Such a
  reference (`Sn 235`, `Th 627`) is tried both as a page and as a verse number,
  so it resolves to the sutta containing the verse. **Dhammapada verse numbers
  do not resolve** (`Dh 183` is read as page 183 and returns nothing): `Dh` takes
  a PTS page, and the chapter of a verse comes from the sutta routes (§14.4).
- **`hits: 0` has three causes that look the same:** a book the table has no PTS
  reference for (the Jātaka, `J i 1`), a page outside every sutta's span, and a
  book abbreviation the parser does not know. It never means "bad request".
- **A query that does not parse** as book + volume + page (`M iii`, `p. 24`)
  falls back to a substring match over `pts_reference`.
- **`sutta_ref` is a substring match on the spaced form.** `MN 110` and `mn 110`
  match; `mn110` does not, and `MN 1` also returns MN 10–19 and MN 100–152. Pick
  the row whose `sutta_ref` equals the query.

### 17.5 Examples

```sh
# PTS → SuttaCentral
curl -s -X POST "localhost:$PORT/reference_search" \
  -H 'Content-Type: application/json' -d '{"query_text":"S v 421"}' \
  | jq -r '.results[] | "\(.sutta_ref)  \(.sc_uid)  \(.title_pali)"'

# SuttaCentral → PTS (exact row only)
curl -s -X POST "localhost:$PORT/reference_search" \
  -H 'Content-Type: application/json' -d '{"query_text":"MN 10","field":"sutta_ref"}' \
  | jq -r '.results[] | select(.sutta_ref == "MN 10") | .pts_reference'

# By Pāli title, diacritics optional
curl -s -X POST "localhost:$PORT/reference_search" \
  -H 'Content-Type: application/json' -d '{"query_text":"satipatthana","field":"title_pali"}' \
  | jq -r '.results[] | "\(.sutta_ref)  \(.sc_uid)  \(.title_pali)"'
```

The first returns SN 56.11 (S v 420–424); the second `M i 55`; the third lists
AN 4.274, DN 22, MN 10 and SN 43.5.

---

## 18. Iti sandhi: which route to use

Pāli joins the quotative particle *iti* to the word before it, and editions
write the join in three ways:

| Written | Example | Means |
|---|---|---|
| quoted | `vuttan'ti`, `dhārayāmī’ti`, `magga’’nti`, `parāyaṇo’”ti` | vuttaṁ / dhārayāmi / maggaṁ / parāyaṇo + iti |
| joined, no quote | `satipaṭṭhānāti`, `abhinandunti`, `viharāmīti` | satipaṭṭhānā / abhinanduṁ / viharāmi + iti |
| spaced | `vuttan ti`, `vuttaṁ ti`, `punabbhavo ti` | vuttaṁ / punabbhavo + iti |

A final `n` before `ti` stands for `ṁ`, and a vowel before `ti` is often
lengthened (`dhārayāmī` for `dhārayāmi`).

**Rule for agents: send the whole form, `ti` included.** Any of the three
spellings works, and a spaced form is sent as one form (`"vuttan ti"`). The
part before `ti` alone loses the iti. For an exact answer use `POST
/lookup_forms` (§18.3, §19.1); for a whole passage, `/gloss_text` with
`"exact": true` keeps every quoted or joined iti form whole (§18.2). To see what
the app's own lookup makes of the form, use `POST /dict_combined_search` and
read its `deconstructor` field (§18.1). §19 says when to use which.

### 18.1 `POST /dict_combined_search`

The query is normalised the DPD way (lowercased, `ṃ` → `ṁ`, quotes handled,
`<word> ti` rejoined) before the lookup. Then:

- If DPD's own entry for the **joined** form (`vuttanti`, `satipaṭṭhānāti`)
  analyses it as `… + iti`, that entry is used as it is: its headwords and its
  split.
- Otherwise the word is resolved as **`<base> + iti`**: the results are the
  base word's, plus `iti`, and the split is `<base> + iti`. So `magga’’nti` is
  maggaṁ + iti and never the verb `magganti`.
- A form written **joined with no quote** that DPD does not analyse as
  `… + iti` is taken as written. `magganti` is the verb *maggati* (third person
  plural), with no split, which is what that spelling means.
- A word ending in a vowel and **`n`** that has no exact match is looked up as
  its `ṁ` spelling before any guess: `vuttan` is `vuttaṁ`, `kin` is `kiṁ`.

Measured with DPD release `v0.4.20260501`. `results`
shows the first few uids; `iti` (`13466/dpd`) is appended to the results of an
iti-sandhi word, after the base word's own:

| `query_text` | First results | `deconstructor` |
|---|---|---|
| `vuttan'ti` | vutta 1.1, 1.2, 2.1, … (11 hits) | `["vuttaṁ + iti"]` |
| `vuttan ti` | same as above | `["vuttaṁ + iti"]` |
| `vuttaṁ ti` | same as above | `["vuttaṁ + iti"]` |
| `vuttan` | vutta 1.1, 1.2, 2.1, … (10 DPD rows, no `iti`; 503 hits with the bold definitions after them) | absent |
| `punabbhavo ti` | punabbhava, iti | `["punabbhavo + iti"]` |
| `satipaṭṭhānāti` | satipaṭṭha, satipaṭṭhāna 1, 2, iti | `["satipaṭṭhānā + iti", "satipaṭṭhāna + iti"]` |
| `abhinandunti` | abhinandi 1, iti | `["abhinanduṁ + iti"]` |
| `viharāmīti` | viharati 1, 2, iti | `["viharāmi + iti"]` |
| `dhārayāmī’ti` | dhārayati 1–4, … | `["dhārayāmi + iti"]` |
| `magga’’nti` | magga 1–4, … | `["maggaṁ + iti"]` |
| `magganti` | maggati, … | absent (the field is omitted) |
| `kho’ti` | kho, iti, … | `["kho + iti"]` |
| `parāyaṇo’”ti` | parāyaṇa 1, 2, 3, 5 | `["parāyaṇo + iti"]` |
| `cā”tipi` | ca 1.1, 1.2, 2.1, … | `["ca + iti + api"]` |

```sh
curl -s -X POST "localhost:$PORT/dict_combined_search" \
  -H 'Content-Type: application/json' -d '{"query_text":"vuttan ti"}' \
  | jq '{deconstructor, first: [.results[0:3][] | {uid, title}]}'
```

For a form DPD does not list, the results can be "starts with" guesses for some
longer word, so a non-empty `results` does not prove that DPD knows the form.
Check the result titles and the `deconstructor` against the query.

### 18.2 `POST /gloss_text`

The gloss splits the text into words first, then looks each word up. How an iti
form comes out depends on how it is written:

- **Quoted, or joined with no quote:** one entry. The `ti` is not given an entry
  of its own. The entry is the **base** word (`vuttan'ti` → `vuttaṁ`,
  `viharāmīti` → `viharāmi`, `magga’’nti` → `maggaṁ`), and the iti is dropped,
  so `deconstructions` is empty. The exceptions are forms DPD lists joined with
  more letters after the iti (`cā”tipi` → `ca + iti + api`) and joined forms DPD
  lists as a headword with an iti split (`satipaṭṭhānāti` keeps both splits).
- **Spaced:** two entries, the word and a separate `ti`. The word is looked up
  alone, and a final `n` is read as `ṁ` (§18.1): in `vuttan ti`, `vuttan` is
  glossed as *vutta* through `vuttaṁ`, and `ti` is its own entry.

In both cases the default gloss entry does not show the iti split. To see it, send the
form to `/dict_combined_search` (§18.1) as written: `"vuttan'ti"`, or
`"<word> ti"` for a word followed by a separate `ti` entry (`"vuttan ti"`).

**With `"exact": true`** (§19.2) a quoted or joined iti form is one token whose
lookup is the **whole** form, iti included, by the §18.3 rule: `vuttan'ti` →
`vuttanti`, found, with `vuttaṁ + iti`; `viharāmīti`, `abhinandunti` and
`satipaṭṭhānāti` likewise; `magga’’nti` → `maggaṁ + iti` (`iti_rule`);
`cā”tipi` → `ca + iti + api`. A **spaced** form is still two tokens, and exact
means exact: `vuttan` is `not_found` (DPD has no form `vuttan`; the `ṁ`
respelling is tier 2), and `ti` is its own `found` token. Send `"vuttan ti"` to
`/lookup_forms` to resolve it exactly.

### 18.3 `POST /lookup_forms`

The same normalisation and the same iti rule as §18.1, over **exact** rows
only: no `ṁ` respelling of a final `n`, no stem match, no guesses. The iti rule
reads the base word with an equality match too, so a `<base> + iti` result never
rests on a guess. A split DPD lists is `"source": "dpd"`; one built by the rule
is `"source": "iti_rule"`, and then `lookup_key` is the base and `headwords` is
empty. Measured on `v0.4.20260501`:

| `input` | `lookup_key` | `status` | headwords | splits |
|---|---|---|---|---|
| `vuttan'ti` | `vuttanti` | found | 10 | `vuttaṁ + iti` (dpd) |
| `vuttan ti` | `vuttanti` | found | 10 | `vuttaṁ + iti` (dpd) |
| `vuttaṁ ti` | `vuttanti` | found | 10 | `vuttaṁ + iti` (dpd) |
| `vuttan` | `vuttan` | not_found | 0 | |
| `punabbhavo ti` | `punabbhavoti` | found | 1 | `punabbhavo + iti` (dpd) |
| `satipaṭṭhānāti` | `satipaṭṭhānāti` | found | 3 | `satipaṭṭhānā + iti`, `satipaṭṭhāna + iti` (dpd) |
| `abhinandunti` | `abhinandunti` | found | 1 | `abhinanduṁ + iti` (dpd) |
| `viharāmīti` | `viharāmīti` | found | 2 | `viharāmi + iti` (dpd) |
| `dhārayāmī’ti` | `dhārayāmīti` | found | 4 | `dhārayāmi + iti` (dpd) |
| `magga’’nti` | `maggaṁ` | deconstructed | 0 | `maggaṁ + iti` (iti_rule) |
| `magganti` | `magganti` | found | 1 (the verb *maggati*) | |
| `kho’ti` | `khoti` | found | 1 | `kho + iti` (dpd) |
| `parāyaṇo’”ti` | `parāyaṇoti` | found | 4 | `parāyaṇo + iti` (dpd) |
| `cā”tipi` | `cātipi` | deconstructed | 0 | `ca + iti + api` (dpd) |

The headwords of a `found` iti form are the ones DPD lists for the joined form,
which are the base word's (`vuttanti`: the ten *vutta* headwords). Unlike
`/dict_combined_search`, `iti` (`13466/dpd`) is **not** added to them; it is in
the split's `iti` component.

**The parses of an iti form's base word are on its base component.** A
result's `form_grammar_raw` is the joined row's grammar, and that row parses
the joined form only as a word in its own right. Usually it has none
(`cittanti`), so the result's `form_grammar_raw` and its headwords'
`form_grammar` are empty. When the joined form is also an inflection, its
grammar is that other reading (`bhavissanti`: `bhavati verb fut 3rd pl`;
`santi`: `atthi verb pr 3rd pl`), 28 such rows on `v0.4.20260501`. Either
way, the base component's `form_grammar_raw` holds the base word's parses
(`cittan'ti`: `cittaṁ`, with `["citta","noun","nt nom sg"]` & co.). An
`iti_rule` result's `form_grammar_raw` is `[]`, since the joined row it
rejected is not shown (§19.1).

---

## 19. Tiers of exactness: widening a lookup

The dictionary routes answer at three levels of exactness. Start at the
narrowest one that fits the job, and widen only for the forms it could not
answer. Each wider tier finds more, and is less sure that what it found is the
word that was asked about.

| Tier | Routes | Matches | A miss means |
|---|---|---|---|
| 1. Exact | `POST /lookup_forms`; `POST /gloss_text` with `"exact": true` | DPD's lookup table, by equality on the normalised form | DPD has no such form |
| 2. The app's lookup | `POST /dict_combined_search`; `POST /gloss_text` (default) | tier 1, then a respelling, the stem, and "starts with" guesses | DPD has nothing like it |
| 3. Wide search | `POST /search`, `"search_area": "Dictionary"`, `"mode": "Headword Match"` or `"Contains Match"` | substrings of headwords (and definitions), in every dictionary | no dictionary has it |

**The order to widen in:**

1. Look the forms up with `/lookup_forms` (up to 5,000 per call). Use the
   `found` and `deconstructed` answers as they are.
2. For a `not_found` form, first check that it is the whole word: an iti form
   split over two tokens (`vuttan` followed by `ti`) goes back to tier 1 as
   one form, `"vuttan ti"` (§18).
3. Send the remaining `not_found` forms to `/dict_combined_search`. What it
   returns for them is, by definition, not DPD's entry for that form: it is a
   respelling or a guess, a **candidate** to check against the text.
4. Use tier 3 only to discover candidates (a rare spelling, a word in a
   non-DPD dictionary), never as an answer.

### 19.1 Tier 1: `POST /lookup_forms`

```jsonc
// request
{ "forms": ["bhagavantaṁ", "cittan'ti", "atthaññe", "vuttan ti", "xyzqq"],
  "exact": true }          // optional, default true; false → 400
// response
{ "dpd_version": "v0.4.20260501",
  "results": [ { "input", "lookup_key", "status", "note"?,
                 "headwords": [ … ], "deconstructions": [ … ],
                 "form_grammar_raw": [ … ] }, … ] }
```

- **Batch size:** up to **5,000** forms per call; 5,001 is a `400`. An empty
  `forms` list is valid and returns `results: []`. The request body may be up
  to 2 MiB (the server's `string` limit), which 5,000 forms fit with room to
  spare; a larger body gets Rocket's own **non-JSON** `400`.
- **Errors** are a JSON `{ "error": "…" }` body: `400` for an unparseable body
  (`"invalid request body: …"`), a missing `forms` (`"missing field: forms"`),
  `"exact": false` (`"only exact lookups are supported; use
  /dict_combined_search for fuzzy matching"`) or too many forms (`"too many
  forms: 5001 (max 5000)"`); `500` for a database error. **A form DPD does not
  know is never an error**: it is a `not_found` result.

- **One result per form, in input order**, duplicates kept. `results[i]`
  answers `forms[i]`.
- **The match is exact.** Each form is normalised as the DPD lookups do it
  (lowercase, `ṃ` → `ṁ`, the quote and iti rules of §18) and matched by
  equality on DPD's `lookup` key, shown back as `lookup_key`. No stem match,
  no "starts with", no respelling: `bhagav` and `vuttan` are `not_found`.
- **Iti sandhi** follows the rule of §18.1, over exact rows only: the joined
  form when DPD analyses it as `… + iti` (`cittan'ti` → `cittanti`, with its
  10 headwords **and** the split `cittaṁ + iti`), otherwise `<base> + iti`
  with `"source": "iti_rule"` (`magga’’nti` → `maggaṁ + iti`, never the verb).
- **Nothing is chosen.** Every headword id and every split of the row is
  returned in DPD's order, with no ranking and no word-selection cache.
- `status`: `found` (one or more headwords; splits may be present too),
  `deconstructed` (splits only), `not_found` (no DPD row with headwords or
  splits), `invalid` (empty after normalisation, more than one word, or a
  `uid:` query; `note` says which).
- Each headword carries `uid` (the numeric `<id>/dpd`, which `/word.json`
  accepts), `lemma_1`, `pos`, `grammar`, `construction[]`, `root_key`,
  `meaning_1`, `matched_as` (`lemma` when the key is the citation form,
  else `inflection`), `root_base`, `root_sign`, `construction_summary` and
  `form_grammar` (see "Form grammar" and "`construction_summary`" below).
  Each split carries `split` (DPD's string), `kind` (`iti_sandhi` /
  `sandhi`), `source` (`dpd` / `iti_rule`) and `components[]` with each
  component's exact headword uids and `form_grammar_raw`.
- Each result carries `form_grammar_raw`: DPD's own parses of the form.
- A root form loses its `√` in normalisation and is `not_found`
  (`√bhaj` → `lookup_key: "bhaj"`): roots are not returned.
- **Caching.** Cache answers on `dpd_version`, not on the app version: the
  same DPD release gives the same answers. But `dpd_version` does not change
  when Simsapa adds fields to these results, so a cache keyed on it alone
  keeps serving the older shape. Also invalidate when `/health`'s
  `app_version` changes.

**Field values.** Every enum is a lowercase snake_case string, safe to `match` on.

| Field | Value | Meaning |
|---|---|---|
| `status` | `found` | One or more headwords. `deconstructions` may be non-empty too (`cittan'ti`). |
| | `deconstructed` | No headwords, at least one split (`atthaññe`, `magga’’nti`). |
| | `not_found` | DPD's lookup table has no row for `lookup_key`, or a row with neither headwords nor splits. |
| | `invalid` | Not a single word form. `note` is one of `"empty after normalisation"` (`""`, or only punctuation), `"more than one word"` (`"a b"`), `"a uid query, not a word form"` (`"uid:iti"`). |
| `matched_as` | `lemma` | `lookup_key` is the headword's citation form (`lemma_clean`): `iti` → `iti`, `bhante` → `bhante`. |
| | `inflection` | Any other form of the headword. A plain comparison, a hint, not a ranking. |
| `kind` | `iti_sandhi` | The split's last component is `iti`. |
| | `sandhi` | Any other split. Informational only. |
| `source` | `dpd` | The split is DPD's, verbatim from the lookup row. |
| | `iti_rule` | Built by the `<base> + iti` rule (§18.3); not in DPD's row. |

`note` is omitted unless the status is `invalid`. `construction` is DPD's
`construction` column split into its lines (`[]` when empty); `root_key`,
`root_base` and `root_sign` are the columns verbatim, `""` when DPD has none.

**Form grammar.** DPD's `lookup` table gives each surface form a list of
`[lemma, class, parse]` triples, one per analysis (`bhagavantaṁ`:
`["bhagavant","noun","masc acc sg"]`, `["bhagavant","adj","nt nom sg"]`, …).
The lemma has no homonym number and the class is broad (`noun`, `verb`,
`adj`), so the API also assigns each triple to the headwords it belongs to.

- `form_grammar_raw` (result) is that list **verbatim**, in DPD's order, as
  JSON arrays, from the lookup row that supplied `headwords`. For a joined iti
  row that is the joined row, which parses the joined form only as a word in
  its own right: none for `cittanti`, `bhavati verb fut 3rd pl` for
  `bhavissanti` (§18.3). An `iti_rule` result (`magga’’nti`) gives `[]`: the joined
  row's grammar is exactly the analysis the rule rejected (`magganti` is the
  verb *maggati*), and the base's grammar is on the base component. `invalid`
  and "no row" give `[]`.
- `form_grammar_raw` (split component) is the component word's **own** exact
  lookup row's list, for `dpd` and `iti_rule` splits alike: `cittan'ti`'s
  `cittaṁ` component lists `["citta","noun","nt nom sg"]` & co.; the `iti`
  component lists `[]` (DPD's `iti` row has no grammar); an unknown component
  gets `[]`. Components get no per-headword assignment: the triples carry the
  lemma and the class, which is what a caller compares.
- `form_grammar` (headword) is the `parse` of every triple of the result's
  `form_grammar_raw` assigned to that headword, in DPD's order, duplicates
  kept. A triple is assigned to a headword when its lemma equals the
  headword's `lemma_clean`, its class equals the headword's `pos` class
  (table below), **and**, for a `noun` triple, the parse's first word (its
  gender) equals the headword's `pos`, so `nt nom sg` of the nt *magga 7* is
  not given to the masc *magga 1*. Verbs are not checked by tense: a `pr`
  headword's forms include every tense (`bhavati` gets `fut 3rd pl`).
- **Each matching homonym gets the triple.** `carissanti`'s
  `["carati","verb","fut 3rd pl"]` goes to all four *carati* headwords: DPD's
  data cannot tell them apart, and choosing is the caller's job.
- A triple no headword matches stays in `form_grammar_raw` only, so an audit
  is a set difference. On `v0.4.20260501` these are exactly the `interr`,
  `prep` and `adv` triples (1,542 of 922,707, 0.17%), whose lemmas are
  headwords under other `pos` values: `katamo` lists
  `["katama","interr","masc nom sg"]`, and the headword *katama* is `pron`.

| headword `pos` | class |
|---|---|
| `masc`, `nt`, `fem` | `noun` |
| `pr`, `aor`, `fut`, `opt`, `imp`, `perf`, `cond`, `imperf` | `verb` |
| every other `pos` (`adj`, `pron`, `pp`, `prp`, `ptp`, `card`, `ordin`, `cs`, `letter`, `root`, `ve`, …) | itself |

`interr`, `prep` and `adv` are deliberately not mapped: no single `pos` is
behind any of them (`interr` lemmas are `adj` 871 times, `pron` 212, and
mixed sets; `prep` lemmas are always mixed; `adv` lemmas are `adj` or
`masc`), so a mapping would assign by guess. The parses are returned as
strings, never parsed into case, number or gender. Rows list up to 39
triples and components repeat their rows' lists, so responses grow with
them, most for exact `/gloss_text` on long texts.

**`construction_summary`** is the construction as DPD's entry displays it,
root base replaced by root + sign: *carati 1.1* (`construction` `cara + ti`)
has `"√car + a + ti"`. It is the bracketed text of DPD's rendered summary
line, taken when the database was built.
`""` means DPD's entry shows no bracket (no `construction`, no `meaning_1`,
or neither). `null` means the installed `dpd.sqlite3` predates the column;
everything else in the result is unaffected.

**Two uid lanes.** A headword `uid` and a component's `uids` are the numeric
`<id>/dpd` form, which `/word.json` resolves to the structured `dpd_headwords`
record. The rendered HTML entry is a `dict_words` row with a different,
hyphenated uid (`dhamma-1-01/dpd`); both are the same word (§13.3). `/word_html?uid=49139/dpd` also
resolves the numeric form.

```sh
curl -s -X POST "localhost:$PORT/lookup_forms" -H 'Content-Type: application/json' \
  -d '{"forms":["bhagavantaṁ","cittan'"'"'ti","atthaññe","bhante","xyzqq"]}' \
  | jq -c '.dpd_version, (.results[] | {input, lookup_key, status,
           headwords: [.headwords[] | "\(.uid) \(.lemma_1) (\(.matched_as))"],
           splits: [.deconstructions[] | .split]})'
```

```jsonc
"v0.4.20260501"
{"input":"bhagavantaṁ","lookup_key":"bhagavantaṁ","status":"found","headwords":["49139/dpd bhagavant 1 (inflection)","49140/dpd bhagavant 2 (inflection)"],"splits":[]}
{"input":"cittan'ti","lookup_key":"cittanti","status":"found","headwords":["26555/dpd citta 1.1 (inflection)","26556/dpd citta 1.2 (inflection)","26557/dpd citta 2.1 (inflection)","26558/dpd citta 2.2 (inflection)","26559/dpd citta 2.3 (inflection)","26560/dpd citta 2.4 (inflection)","26561/dpd citta 2.5 (inflection)","26562/dpd citta 2.6 (inflection)","26563/dpd citta 1.3 (inflection)","77246/dpd cittā (inflection)"],"splits":["cittaṁ + iti"]}
{"input":"atthaññe","lookup_key":"atthaññe","status":"deconstructed","headwords":[],"splits":["atthi + aññe"]}
{"input":"bhante","lookup_key":"bhante","status":"found","headwords":["49400/dpd bhanta 1 (inflection)","49401/dpd bhanta 2 (inflection)","49402/dpd bhanta 3 (inflection)","49408/dpd bhante (lemma)","49552/dpd bhavant (inflection)","49699/dpd bhāti 1.1 (inflection)"],"splits":[]}
{"input":"xyzqq","lookup_key":"xyzqq","status":"not_found","headwords":[],"splits":[]}
```

**`cittan'ti` is `found` with a split, and that is not a bug.** DPD's row for
`cittanti` lists ten headwords (it is also an inflected form of *citta*) **and**
the split `cittaṁ + iti`. Nothing is chosen, so both come back: the caller
decides which reading the text means.

One headword in full (`.results[0].headwords[0]`), its result's
`form_grammar_raw`, and one split (`.results[2].deconstructions`, the
components' `form_grammar_raw` cut short):

```jsonc
{ "uid": "49139/dpd",
  "lemma_1": "bhagavant 1",
  "pos": "masc",
  "grammar": "masc, from bhaga",
  "construction": ["√bhaj > bhag + a + vant", "bhaga + vant"],
  "root_key": "√bhaj 1",
  "meaning_1": "Sublime One; Blessed One; Fortunate One; Auspicious One; epithet of the Buddha",
  "matched_as": "inflection",
  "root_base": "",
  "root_sign": "a",
  "construction_summary": "√bhaj + a + vant",
  "form_grammar": ["masc acc sg"] }

"form_grammar_raw": [["bhagavant","noun","masc acc sg"],["bhagavant","adj","nt nom sg"],
                     ["bhagavant","adj","masc acc sg"],["bhagavant","adj","nt acc sg"]]

[ { "split": "atthi + aññe",
    "kind": "sandhi",
    "source": "dpd",
    "components": [
      { "word": "atthi", "uids": ["2736/dpd","2737/dpd","2738/dpd","2757/dpd"],
        "form_grammar_raw": [["atthi","verb","pr 3rd sg"],["atthi","verb","pr 3rd pl"],
                             ["atthi","noun","masc nom sg"], …] },
      { "word": "aññe",  "uids": ["1497/dpd","1498/dpd","1601/dpd","1631/dpd","1660/dpd","74340/dpd","80675/dpd"],
        "form_grammar_raw": [["añña","pron","masc nom pl"],["añña","pron","nt nom pl"], …] } ] } ]
```

A component's `uids` are its own exact lookup row's headwords; a component DPD
does not know gets `[]`, never a guess.

The form grammar of a few forms, one line per result:

```sh
curl -s -X POST "localhost:$PORT/lookup_forms" -H 'Content-Type: application/json' \
  -d '{"forms":["bhagavantaṁ","carissanti","katamo","cittan'"'"'ti"]}' \
  | jq -c '.results[] | {input, raw: (.form_grammar_raw | length),
           headwords: [.headwords[] | "\(.uid) \(.lemma_1) [\(.pos)] \(.form_grammar) \(.construction_summary)"],
           components: [.deconstructions[].components[] | "\(.word): \(.form_grammar_raw | length)"]}'
```

```jsonc
{"input":"bhagavantaṁ","raw":4,"headwords":["49139/dpd bhagavant 1 [masc] [\"masc acc sg\"] √bhaj + a + vant","49140/dpd bhagavant 2 [adj] [\"nt nom sg\",\"masc acc sg\",\"nt acc sg\"] √bhaj"],"components":[]}
{"input":"carissanti","raw":1,"headwords":["26276/dpd carati 1.1 [pr] [\"fut 3rd pl\"] √car + a + ti","26277/dpd carati 2.1 [pr] [\"fut 3rd pl\"] √car + a + ti","26278/dpd carati 2.2 [pr] [\"fut 3rd pl\"] √car + a + ti","26279/dpd carati 2.3 [pr] [\"fut 3rd pl\"] √car"],"components":[]}
{"input":"katamo","raw":1,"headwords":["19645/dpd katama [pron] [] ka + tama"],"components":[]}
{"input":"cittan'ti","raw":0,"headwords":["26555/dpd citta 1.1 [nt] [] √cit + ta","26556/dpd citta 1.2 [nt] [] √cit + ta","26557/dpd citta 2.1 [adj] [] √citt + a","26558/dpd citta 2.2 [adj] [] √citt + a","26559/dpd citta 2.3 [nt] [] √citt + a","26560/dpd citta 2.4 [masc] [] √citt + a","26561/dpd citta 2.5 [masc] [] √citt + a","26562/dpd citta 2.6 [masc] [] √citt + a","26563/dpd citta 1.3 [nt] [] √cit","77246/dpd cittā [fem] [] √citt + a + ā"],"components":["cittaṁ: 9","iti: 0"]}
```

`katama`'s only triple is `interr`, so it is in `raw` and in no headword's
`form_grammar`. `cittan'ti`'s joined row has no grammar; the parses are on
its `cittaṁ` component.

### 19.2 Tier 1 for a passage: `/gloss_text` with `"exact": true`

`{ "text": "…", "options": { "exact": true } }` returns, for every word of each
paragraph, in order, a token `{ token_index, original_word, clean_word,
context, lookup }`, where `lookup` is the §19.1 result for `original_word`.
Nothing is deduplicated, skipped or chosen, and `token_index` runs 0..n-1 in
each paragraph, so the tokens zip against the words by position. Each
`lookup` carries every §19.1 field, including `form_grammar`,
`form_grammar_raw` and `construction_summary`.

- The words are the gloss tokeniser's, with annotations (sutta references,
  verse numbers) stripped. A quoted or joined iti form is one token and keeps
  its `ti` (`bhavissan'ti` → `bhavissanti`, found, with `bhavissaṁ + iti`).
- A **spaced** iti form is two tokens: `vuttan`, `not_found`, then `ti`. Widen
  it by sending `"vuttan ti"` to `/lookup_forms`, which stays exact.
- `no_duplicates_globally`, `skip_common` and `common_words` do not apply in
  exact mode; sending any of them with a non-default value is a `400`.
- A database error is a `500` with `{ "error" }`; no token is ever dropped.
- `original_word` is the word as written, capitals and inner quote marks
  kept, without the punctuation around it (`Bhavissan'ti,` → `Bhavissan'ti`).
  It is the form looked up, so `lookup.input` equals it.

```jsonc
{ "success": true,
  "exact": true,
  "dpd_version": "v0.4.20260501",
  "paragraphs": [
    { "paragraph_index": 0,
      "tokens": [
        { "token_index": 0, "original_word": "Evaṁ", "clean_word": "evaṁ",
          "context": "<b>Evaṁ</b>, bhante, iti kho āvuso bhavissan'ti.",
          "lookup": { /* a /lookup_forms result, §19.1 */ } }, … ] } ] }
```

`paragraph_index` counts the non-empty `\n\n`-separated paragraphs from 0;
`token_index` restarts at 0 in each. `clean_word` is the tokeniser's own
cut-down form (the base of an iti word) and is informational: the lookup is
always on the whole word.

```sh
curl -s -X POST "localhost:$PORT/gloss_text" -H 'Content-Type: application/json' \
  -d '{"text":"Evaṁ, bhante, iti kho āvuso bhavissan'"'"'ti.","options":{"exact":true}}' \
  | jq -c '.paragraphs[].tokens[] | {token_index, original_word, clean_word,
           lookup_key: .lookup.lookup_key, status: .lookup.status,
           headwords: (.lookup.headwords | length), splits: [.lookup.deconstructions[].split]}'
```

```jsonc
{"token_index":0,"original_word":"Evaṁ","clean_word":"evaṁ","lookup_key":"evaṁ","status":"found","headwords":2,"splits":[]}
{"token_index":1,"original_word":"bhante","clean_word":"bhante","lookup_key":"bhante","status":"found","headwords":6,"splits":[]}
{"token_index":2,"original_word":"iti","clean_word":"iti","lookup_key":"iti","status":"found","headwords":1,"splits":[]}
{"token_index":3,"original_word":"kho","clean_word":"kho","lookup_key":"kho","status":"found","headwords":1,"splits":[]}
{"token_index":4,"original_word":"āvuso","clean_word":"āvuso","lookup_key":"āvuso","status":"found","headwords":1,"splits":[]}
{"token_index":5,"original_word":"bhavissan'ti","clean_word":"bhavissaṁ","lookup_key":"bhavissanti","status":"found","headwords":9,"splits":["bhavissaṁ + iti"]}
```

### 19.3 Tier 2: `/dict_combined_search` and the default `/gloss_text`

These run the app's own DPD lookup, the one behind the Dictionary tab and
WordSummary. For a word with an exact match, the DPD rows are the same as in
tier 1. When there is none, it goes on, in this order:

1. **The `ṁ` spelling of a final `n`**: `vuttan` → `vuttaṁ`, `kin` → `kiṁ`.
   This is usually right, because the texts write `ṁ` as `n` before `ti` or a
   vowel, but it is not DPD's entry for `vuttan`.
2. **The stem match**: headwords with the same stem. `bhagav` gets
   `bhagavant` this way.
3. **"Starts with" guesses**: a break-down guessed from a longer key, then
   headwords that start with the form. `bhagav` also gets the split
   `bhaga + vata`, and `atthañ` gets `atthaṁ + ca` and its `attha` headwords.
   A guess belongs to some other word.

Nothing in the `/dict_combined_search` response says which step answered, so
compare the result titles and the `deconstructor` with the form, or ask tier 1:
a form that is `not_found` there has only respellings and guesses here.
`hits` counts every row of the page, including the commentary bold-definition
rows that follow the DPD rows (`vuttan`: 10 DPD rows, 503 hits); the DPD rows
are the ones whose `uid` ends in `/dpd`.

The default `/gloss_text` uses the same lookup for each word, and also
**chooses**: it drops repeats and, with `skip_common`, common words; it applies
the app's cached sense selections (`selected_index`); and it cuts a quoted iti
form to its base (`vuttan’ti` → `vuttaṁ`), so the split is not shown (§18.2).
Use it to build a reading aid, not to check forms.

### 19.4 Tier 3: `Headword Match` and `Contains Match`

`POST /search` with `"search_area": "Dictionary"` and `"mode": "Headword
Match"` matches headwords containing the form, in every installed dictionary;
`"Contains Match"` matches the form inside headwords or definitions. For `vuttan`,
Headword Match returns 500 hits (`vuttanayatta`, `vuttanaya`, `vuttaniyāma`, …)
and Contains Match 12,795. These are for finding candidates, such as a spelling
DPD lacks or a word only another dictionary has; confirm a candidate with
tier 1 before using it.

