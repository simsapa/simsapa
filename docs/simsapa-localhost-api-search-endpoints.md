# Simsapa Localhost API search endpoints

The app runs a local HTTP server (Rocket, `bridges/src/api.rs`, bound to
`127.0.0.1:<api_port>`) used by the browser extension and other local clients.
The default port is 4848.

```sh
curl -s -X POST "localhost:4848/suttas_fulltext_search" \
  -H 'Content-Type: application/json' \
  -d '{"query_text":"vedanā aniccā","suttas_lang":"pli","page_num":0,"page_len":20}' | python3 scripts/simsapa_fmt.py
```

    # 1685 hit(s); showing 20
    
    [1] SN 18.5 — Vedanāsutta  (sn18.5/pli/ms)
        «aniccā» bhante sotasamphassajā «vedanā» pe ghānasamphassajā «vedanā» jivhāsamphassajā «vedanā» kāyasamphassajā «vedanā» manosamphassajā «vedanā» niccā vā «aniccā» va ti «aniccā» bhante evaṁ
    
    [2] SN 18.5 — 5. Vedanāsuttaṁ  (sn18.5/pli/cst)
        sotasamphassajā «vedanā» pe ghānasamphassajā «vedanā» jivhāsamphassajā «vedanā» kāyasamphassajā «vedanā» manosamphassajā «vedanā» niccā vā «aniccā» va ti «aniccā» bhante pe evaṁ passaṁ rāhula
    
    [3] SN 22.90 — 8. Channasuttaṁ  (sn22.90/pli/cst)
        rūpaṁ kho āvuso channa «aniccaṁ» «vedanā aniccā» saññā «aniccā» saṅkhārā «aniccā» viññāṇaṁ «aniccaṁ» rūpaṁ anattā «vedanā» saññā saṅkhārā viññāṇaṁ anattā sabbe
    ...

This document covers the **whole route surface**. The four **search** endpoints
and the **sutta/dictionary retrieval** routes are documented in detail (request /
response JSON, parameter structs, curl examples) in §1–§13; **every other route**
is catalogued with its purpose in the complete route reference (§14), with the
remaining request/response structs in §15.

All search routes reuse the in-app search path
(`SearchQueryTask::new` + `results_page(page_num)` + `total_hits()`), so the
returned results carry exactly the same producer-owned, **non-nested**
`<span class='match'>` highlighting, `is_snippet` markers, per-occurrence
snippet expansion, and snippet-exclusion behaviour as the in-app results. See
[search-snippet-highlight-pipeline.md](./search-snippet-highlight-pipeline.md)
for how the snippet/highlight stages work; this doc only covers the API plumbing.

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

# --- Dictionary ---------------------------------------------------------------
# 1. Search DPD. Copy a .results[].uid (e.g. "dhamma-1-01/dpd" or numeric "34626/dpd").
curl -s -X POST "localhost:$PORT/dict_combined_search" \
  -H 'Content-Type: application/json' -d '{"query_text":"dhamma"}' | jq '.results[].uid'
# 2a. Structured record as JSON (for glossary export / grammar fields):
curl -s "localhost:$PORT/words/dhamma-1-01/dpd.json" | jq '.[0]'
# 2b. …or the rendered entry HTML:
curl -s "localhost:$PORT/get_word_html_by_uid/web/dhamma-1-01/dpd"
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

### 0.1 Recommended setup: a formatter script + a permission allowlist

The raw responses are verbose HTML/JSON. For autonomous use, pair `curl` with a
small **formatter script** that reads a response on stdin and prints a compact,
grep-friendly summary (hit count, `uid`s, plain-text snippets with matches marked,
the deconstructor split, the dictionary fields). This keeps the agent's context
small and makes results `grep`-able.

A ready-to-use example can be found in the Simsapa repository:
[`scripts/simsapa_fmt.py`](../scripts/simsapa_fmt.py). It is a *formatter only*
(it does no network I/O — `curl` does the request), and it auto-detects every
shape these routes return: search results, word records, the `verbose=1` word
envelope, the `/health` and `/sutta_and_dict_search_options` snapshots, and
rendered sutta/word HTML (tags stripped to plain text). Typical pipeline:

```sh
curl -s -X POST "localhost:$PORT/suttas_fulltext_search" \
  -H 'Content-Type: application/json' \
  -d '{"query_text":"vedanā aniccā","suttas_lang":"pli","page_num":0,"page_len":20}' | python3 scripts/simsapa_fmt.py --no-color
```

Matched terms are wrapped in `«…»` markers and, when stdout is a terminal,
**highlighted in color** (bold yellow). The color is automatic — it is
suppressed when the output is piped or redirected so it never pollutes
`grep`-ed or captured text. Useful flags:

- `--no-color` — disable the ANSI color highlight even on a terminal (the
  `«…»` markers are kept).
- `--no-marks` — drop the `«…»` markers entirely (also disables color).
- `--max N`, `--snippet-len N`, `--no-snippet`, `--raw` — cap rows, truncate
  snippets, omit snippets, or pretty-print the parsed JSON unchanged.

**Fulltext Search examples:**

The fulltext search uses [tantivy's query syntax](https://docs.rs/tantivy/latest/tantivy/query/struct.QueryParser.html). The 'must' (+) and 'negative' (-) terms are particulary useful for filtering results.

Words don't have to be exactly near each other, e.g. **so ce evam vadeyya** will also find **so ce** bhikkhu **evaṁ vadeyya**

Prefixing a term with + and - can control "Must" or "Must not" include.

**santam padam abhisamecca** -- each term may be included, but ok if not all are found.

**santam padam +abhisamecca** -- 'abhisamecca' must be included, even if the others may be absent.

**santam padam -abhisamecca** -- 'abhisamecca' must not be included.

Fulltext matches Pāli declensions but doesn't do partial word matches, so **upasan** doesn't find anything (not a valid declension stem) until you type **upasankama**.

The *Contains Match* is for exact partial matches.

If you are an agent that wants to search **autonomously and safely**, write such
a formatter (copy or adapt `scripts/simsapa_fmt.py`) and recommend the user
add **only** these to your tool-permission allowlist:

- `curl` (or the localhost host) — to call the API,
- `grep` — to filter the formatted output,
- the formatter script itself (e.g. `python3 scripts/simsapa_fmt.py`).

All three are read-only with respect to the user's data: `curl` hits a
`127.0.0.1`-bound local server, the formatter only reshapes stdin, and `grep`
only reads. Allow-listing this trio lets the agent run the whole
search → copy `uid` → fetch-full-text loop without a permission prompt per call,
while keeping everything else gated.

---

## 1. The four endpoints

| Route | Default mode (non-reference) | Area | Deconstructor |
|-------|------------------------------|------|---------------|
| `POST /search` | area-specific (see §4) | any (request-driven) | Dictionary only |
| `POST /suttas_fulltext_search` | `FulltextMatch` (tantivy) | Suttas | `None` |
| `POST /suttas_contains_search` | `ContainsMatch` (literal) | Suttas | `None` |
| `POST /dict_combined_search` | `DpdLookup` | Dictionary | yes |

The three Suttas/general routes plus `/dict_combined_search` are mounted in the
`routes![...]` list in `start_webserver()`.

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

- `snippet_exclude` is an **already-split array**; CSV-splitting is a QML/UI
  concern, not done API-side.
- The language/source filters treat the placeholder values `"Languages"` /
  `"Language"` (and `"Dictionaries"` / `"Dictionary"` for the source) — and the
  empty string — as **no filter**.

## 3. Exact mode / area serde names

Request strings must match the `SearchMode` / `SearchArea` serde labels exactly
(`backend/src/types.rs`); an unrecognized value on `/search` returns **HTTP 400**.

`mode`:
`"Combined"`, `"Fulltext Match"`, `"Contains Match"`, `"Headword Match"`,
`"Title Match"`, `"DPD ID Match"`, `"DPD Lookup"`, `"Uid Match"`,
`"RegEx Match"`.

`search_area`: `"Suttas"`, `"Library"`, `"Dictionary"`.

## 4. `POST /search` mode / area resolution

- `search_area` defaults to `"Suttas"` when omitted; an unknown value → 400.
- `mode` defaults are **area-specific** (matching the `SearchBarInput.qml`
  dropdown index 0): Suttas/Library → `"Fulltext Match"`, Dictionary →
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
auto-detect: if `query_text_to_uid_field_query(query_text)` returns a
`uid:`-prefixed query (e.g. for `"sn56.11"`, `"MN 44"`, `"dhp182"`), the route
runs `UidMatch` instead of its fallback mode. `/dict_combined_search` does the
same for dictionary UID patterns. `/search` does **not** do this (mode is
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
total (`SearchQueryTask::total_hits()`) and is **unchanged** by snippet expansion
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
does not compute `show_header` / `find_query` (those are derived QML-side and are
not stored on `SearchResult`); a client can recompute them from the returned rows.

## 8. Lazy, mode-gated fulltext searcher init

The webserver runs on a thread in the **same process** as the GUI
(`cpp/gui.cpp` spawns `start_webserver`), so it shares the one process-global
`FULLTEXT_SEARCHER` (`backend/src/lib.rs`). The query path does **not** self-init
the searcher — `with_fulltext_searcher(...)` returns `None` when uninitialized
and the query returns **silent-empty** results.

The shared `run_search` helper therefore calls
`simsapa_backend::init_fulltext_searcher()` **only** when the resolved mode needs
the Tantivy index (`FulltextMatch` or `Combined`), immediately before running the
query. `init_fulltext_searcher()` is idempotent (no-op if already loaded), so in
steady state — any realistic curl / browser-extension request, long after the UI
finished starting — it does nothing. It does real work only in the edge case
where QML init never ran (the case that would otherwise return silent-empty).

Init is **not** eager at `start_webserver()`: right after the webserver thread is
spawned, `gui.cpp` runs `reconcile_dict_indexes_blocking_c()` which performs
Tantivy **writes** then `reinit_fulltext_searcher()`. Opening a reader eagerly on
the API thread at startup would contend with those writes and pay the cold
index-open cost even for clients that never query fulltext. Concurrency is safe:
searcher access is behind an `RwLock`; the API and QML threads are concurrent
readers.

## 9. Shared helpers (`bridges/src/api.rs`)

- `parse_search_mode` / `parse_search_area` — request string → enum (exact serde
  labels), `None` on unknown (→ 400 on `/search`).
- `build_search_params(request, mode, area)` — builds the `SearchParams`
  literal: area-aware language/source filters, `page_len` (default 20),
  `show_all_snippets` / `snippet_exclude` from the request, defaults for the rest.
- `run_search(dbm, query_text, params, area, page_num, deconstructor)` — lazy
  mode-gated searcher init, then `SearchQueryTask` + `results_page` +
  `total_hits`, returning `ApiSearchResult`; logs and returns empty on error.
- `run_suttas_search(request, dbm, fallback_mode)` — shared body for the two
  named Suttas routes: reference → `UidMatch` auto-detect, else `fallback_mode`.
- `run_search_with_uid_fallback(...)` — wraps `run_search` with the self-correcting
  0-hit → `fallback_mode` re-run used by the Suttas routes (§5).
- `run_dict_combined_with_fallback(...)` — the dictionary self-correcting chain
  (0-hit auto-`UidMatch` → normalized `UidMatch` → `DpdLookup`); used by
  `/dict_combined_search` and `/search`'s Dictionary `Combined` path (§5).
- `resolve_word_uid` (backend `AppData`) — the shared, tolerant word-uid resolver
  behind the JSON and HTML word routes (§13.3); `normalize_human_word_uid`
  (backend `helpers`) is its pure display-form → canonical-uid normalizer.
- `word_json_response` / `word_html_response` / `sutta_html_response` — shared
  bodies for the word/sutta retrieval routes (resolver + 404-on-miss + the
  `?verbose=1` envelope, §13).

## 10. The port (default 4848)

The server binds to `127.0.0.1:<api_port>`. The port is resolved at startup
(`backend/src/lib.rs`):

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
  "counts": {                         // row counts in the live DBs
    "suttas": 21359,
    "dict_words": 216009,
    "dpd_headwords": 88864
  },
  "sutta_languages": ["en","pli"],    // same lists as /sutta_and_dict_search_options
  "dict_sources": ["dpd","dppn", …]
}
```

- **`fulltext_searcher_ready`** reflects the lazy, mode-gated searcher init of §8:
  it is `false` on a fresh process and flips to `true` after the **first**
  `FulltextMatch`/`Combined` query (or a QML `load_searcher`). It stays `true`
  thereafter (the searcher is process-global). To watch the flip:

  ```sh
  curl -s "localhost:$PORT/health" | jq '.fulltext_searcher_ready'   # false
  curl -s -X POST "localhost:$PORT/suttas_fulltext_search" \
    -H 'Content-Type: application/json' -d '{"query_text":"dukkha"}' >/dev/null
  curl -s "localhost:$PORT/health" | jq '.fulltext_searcher_ready'   # true
  ```

- **`counts`** are per-DB row counts. Each is resilient: a real `0` means the DB
  is loaded but empty / not installed (consistent with
  `fulltext_searcher_ready: false`), while `null` means the count query itself
  errored — the rest of `/health` is still returned.
- **`dict_sources`** is the authoritative list of installed dictionaries — check
  it before expecting `/words/<uid>.json` or `dict_dict` filters to resolve a
  given source (the §13.3 verbose-miss `hint` points here for exactly this
  reason).

## 11. Response fields (`SearchResult`)

Each element of `results` is a `SearchResult` (`backend/src/types.rs`); the
fields most clients use:

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
caveat) — use the GET render routes (`bridges/src/api.rs`), not the search
endpoints. These return rendered HTML; strip the tags to get plain text.

| Route | Returns |
|-------|---------|
| `GET /get_sutta_html_by_uid/<window_id>/<uid..>` | **Full sutta HTML** for a uid. `<window_id>` is any client id (e.g. `web`). Optional `?anchor=<id>` shows reference anchors and jumps to a segment. Applies verse-ref / `/pli/ms` / range normalization (§14.4). **404 on a genuine miss.** |
| `GET /get_word_html_by_uid/<window_id>/<uid..>` | Full dictionary-word entry HTML for a word uid (e.g. `dhamma-1-01/dpd`). Resolves the same tolerant set of uid forms as the JSON route (§13.3); the numeric headword form `34626/dpd` now renders a full page (was blank). **404 on a genuine miss.** |
| `GET /word_html?window_id=<id>&uid=<uid>` | **Query-param twin** of `get_word_html_by_uid`. Same HTML, but the uid is a query parameter so its `/` may be `%2F`-encoded (or raw — encoding-agnostic). Both params required (missing either → HTTP 422). Use when a client must percent-encode the uid. |
| `GET /sutta_html?window_id=<id>&uid=<uid>&[anchor=<id>]` | **Query-param twin** of `get_sutta_html_by_uid`. Same sutta HTML, uid as an encoding-agnostic query parameter (`%2F` or raw `/`). `window_id` + `uid` required, `anchor` optional. Also applies verse-ref / `/pli/ms` / range normalization (§14.4). |

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

The resolver (`AppData::resolve_word_uid`, shared with the HTML route) now
resolves the same tolerant set of uid forms for both routes — human/display,
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
the `dict_words` `DictWord` model) — not the `SearchResult` of §11.

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

## 14. Complete route reference

Every route mounted in `start_webserver()`'s `routes![...]` (`bridges/src/api.rs`).
Routes detailed earlier are cross-referenced; the rest are listed here with their
purpose. Many of the GUI-navigation routes are **side-effecting**: they fire a
`cxx-qt` `callback_*` into the running GUI (open a window/tab, navigate, toggle a
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
| `GET /sutta_and_dict_search_options` | Filter option lists (`sutta_languages[]`, `dict_languages[]`, `dict_sources[]`) | §10; struct `SearchOptions` §15 |
| `GET /get_sutta_html_by_uid/<window_id>/<uid..>?<anchor>&<layout>&<columns>` | Full rendered sutta HTML (text retrieval); 404 on miss; optional display params | §13, §14.5 |
| `GET /sutta_html?window_id=<id>&uid=<uid>&[anchor=<id>]&[layout=…]&[columns=…]` | Query-param twin of `get_sutta_html_by_uid`; uid encoding-agnostic (`%2F` ok); 404 on miss; optional display params | §13, §14.5 |
| `GET /sutta_content_block?uid=<uid>&[layout=…]&[columns=…]&[show_references=…]` | Just the sutta content-block HTML (no page chrome) for in-page layout/column re-renders; 400 bad layout, 404 unknown uid/column; 200 carries the resolved column list in the `X-SSP-Columns` header | §14.5 |
| `GET /translations_for_sutta?uid=<uid>` | JSON array of the other texts sharing the sutta's reference (column-bar dropdowns): `item_uid`, `sutta_title`, `sutta_ref`, `language`, `author`, `has_content_json` | §14.5 |
| `POST /save_sutta_display_settings` | Persist the `sutta_display` defaults (cogwheel menu "Save as default"). Body: `SuttaDisplayDefaults` JSON; 200 on success, 400/422 on malformed body | §14.5 |
| `GET /get_word_html_by_uid/<window_id>/<uid..>` | Full rendered dictionary-word HTML; 404 on miss | §13 |
| `GET /word_html?window_id=<id>&uid=<uid>` | Query-param twin of `get_word_html_by_uid`; uid encoding-agnostic (`%2F` ok); 404 on miss | §13 |
| `GET /words/<uid>.json` | Full dictionary-word record as JSON (path form; raw `/` only); 404 + `[]` on miss | §13.3 |
| `GET /word.json?uid=<uid>&[verbose=1]` | Full dictionary-word record as JSON (query form; uid encoding-agnostic). Default bare array, 404 + `[]` on miss; `verbose=1` → diagnostic envelope | §13.3 |
| `GET /get_book_spine_item_html_by_uid/<window_id>/<spine_item_uid..>` | Full rendered Library-book chapter HTML, by spine-item uid | — |
| `GET /book_pages/<book_uid>/<resource_path..>` | Rendered Library-book page HTML, by in-book resource path | — |
| `GET /sutta_titles_flat_completion_list` | Autocomplete list of sutta titles. **Placeholder — returns `[]`** (the extension uses a bundled list) | — |
| `GET /dict_words_flat_completion_list` | Autocomplete list of dictionary words. **Placeholder — returns `[]`** | — |

### 14.2 GUI navigation (side-effecting; open/navigate windows)

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
| `GET /health` | JSON diagnostics snapshot: `app_version`, `api_port`, `db_paths`, `fulltext_searcher_ready`, `counts`, `sutta_languages`, `dict_sources` (see §10). |
| `GET /shutdown` | Shut the webserver down (`Shutdown::notify`). Used by `shutdown_webserver` / `shutdown_webserver_tcp`. |
| `GET /app-assets-list` | Debug HTML listing of the SIMSAPA_DIR and internal-storage directory trees. |
| `GET /assets/<path..>` | Serve a bundled static asset (CSS/JS/fonts/images/pdf-viewer) from the embedded `assets/` dir. |
| `GET /favicon.ico` | Serve the app icon as the favicon. |
| `GET /book_resources/<book_uid>/<path..>` | Serve a binary resource (image/css/font/pdf) imported with a Library book, from the DB. |
| `GET /dict_resources/<dict_id>/<path..>` | Serve a binary resource imported with a StarDict dictionary, keyed by numeric `dict_id`. |
| `GET /get_pdf_viewer/<book_uid>` | Redirect/loader HTML that opens the bundled PDF.js viewer pointed at a book's `document.pdf` (browser testing). |
| `POST /logger` | Write a message to the app log. Body: `LoggerRequest` (§15). |
| `POST /copy_to_clipboard` | Copy text to the system clipboard (`text/plain`). Body: `CopyToClipboardRequest` (§15). |
| `POST /open_external_url` | Open a URL in the system browser. Body: `OpenExternalUrlRequest` (§15). |

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

The search routes' reference auto-detect (§5) is a different mechanism
(`query_text_to_uid_field_query`, query-string → `uid:` field query).

### 14.5 Sutta display parameters and routes (multi-column view)

The multi-translation side-by-side feature (see
`docs/sutta-display-settings-and-multi-column-view.md`) adds optional display
parameters to the sutta render routes plus three dedicated routes. Shared
parameter semantics (parsed by `parse_display_overrides` in
`backend/src/sutta_display.rs`; absent parameters fall back to the persisted
`sutta_display` defaults in `AppSettings`):

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
  errors → 500 (shared `render_error_status` mapping in `api.rs`); an
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
`SearchResult` rows (§11). The remaining routes use these smaller structs
(`bridges/src/api.rs`); all are plain JSON objects.

```rust
// GET /sutta_and_dict_search_options  → response
struct SearchOptions {
    sutta_languages: Vec<String>,
    dict_languages: Vec<String>,
    dict_sources: Vec<String>,
}

// POST /lookup_window_query  ← request
struct LookupWindowRequest { query_text: String }

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

`/words/<uid>.json` returns `Vec<serde_json::Value>` (the raw DB row, §13.3); the
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
