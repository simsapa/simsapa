# Localhost API: implementation notes

The client-facing reference for the routes and the `simsapa` commands is
[simsapa-cli-and-localhost-api.md](./simsapa-cli-and-localhost-api.md). That
file is **compiled into the `simsapa` binary** (`simsapa docs`) and read by
agents outside this repository, so it must stay self-contained: no links to
other docs, no source paths, no `CLAUDE.md`. Implementation detail goes here.
`backend_cli/src/commands/docs.rs` has a test that fails on a repo reference in
the bundled doc.

Crate layout, builds and npm packaging are in
[backend-cli-and-npm-package.md](./backend-cli-and-npm-package.md).

## Where the routes live

- **Shared data routes:** the Qt-free `simsapa_api` crate, `api/src/routes/`,
  registered in `shared_routes()` in `api/src/lib.rs` (its doc comment is the
  authoritative inventory, and a route-count test guards it).
  `simsapa_api::build_rocket()` mounts them; `simsapa serve` calls it as is.
- **GUI, AI and static-asset routes:** `bridges/src/api.rs`.
  `start_webserver()` calls `build_rocket()` and mounts these on top. The AI
  engine behind `/word_selection_ws` is `bridges/src/ai_engine.rs`.
- **Request and result types:** `SearchMode` / `SearchArea` serde labels and
  `SearchResult` in `backend/src/types.rs`; the smaller request/response
  structs of §15 next to their routes.

| Route / behaviour | Code |
|---|---|
| Search routes, mode defaults | `api/src/routes/search.rs`; the default mode mirrors `SearchBarInput.qml`'s dropdown index 0 |
| Named-route reference auto-detect | `query_text_to_uid_field_query(query, &SearchArea::…)` (backend `helpers`) |
| Word routes | `AppData::resolve_word_uid` (shared by JSON and HTML), `normalize_human_word_uid` (backend `helpers`) |
| `/sutta.json`, `/sutta_bilara.json` | `backend/src/sutta_json.rs`: `get_sutta_content_json`, `resolve_sutta_strict`, `get_sutta_bilara_json`. The column is never parsed: `serde_json` is built without `preserve_order`, so a `serde_json::Value` would sort the segment ids |
| Verse references | `convert_verse_ref_to_sutta_uid` (backend `helpers`), shared with the sutta routes of §14.4 |
| Inline variant readings and notes | `bilara_text_to_segments()` in `backend/src/helpers.rs` |
| Sutta display parameters | `parse_display_overrides` in `backend/src/sutta_display.rs`; defaults from `AppSettings.sutta_display`; errors through `render_error_status`. See [sutta-display-settings-and-multi-column-view.md](./sutta-display-settings-and-multi-column-view.md) |
| `/gloss_text` | `api/src/routes/gloss.rs`; core `helpers::process_all_paragraphs()` / `process_all_paragraphs_exact()`, shared with the GlossTab and `simsapa gloss`. See [gloss-ai-word-selection.md](./gloss-ai-word-selection.md) |
| `/reference_search` | `pts_reference_search::search()` in `backend/src/pts_reference_search.rs` over the embedded `assets/sutta-reference-converter.json`. The route parses the table on its first call: the window otherwise does that when it opens, and the search panics on an unparsed table |
| `/navigation_timeline`, `/open_sutta_navigation` | [sutta-navigation-tree.md §2h](./sutta-navigation-tree.md) |
| `construction_summary` | [dpd-construction-summary.md](./dpd-construction-summary.md) |
| Iti sandhi rules | [pali-sandhi-and-quote-marks.md](./pali-sandhi-and-quote-marks.md); DPD lookup rule and the `ṁ` respelling: [search-snippet-highlight-pipeline.md §9d, §9f](./search-snippet-highlight-pipeline.md) |
| `/health` fulltext block | `fulltext_status.rs`; see [fulltext-index-storage-and-file-locking.md](./fulltext-index-storage-and-file-locking.md) |
| `/health` `sc_data_version` | `AppdataDbHandle::sc_data_version()` reads the `app_settings` rows `sc_data_date` / `sc_data_commit`, written by `record_sc_data_version()` in `bootstrap_cli/src/bootstrap/appdata.rs` from `git log -1` of the `sc-data` checkout, after the SuttaCentral import. The About window shows it through `SuttaBridge.sc_data_version()` |
| `/shutdown` | `Shutdown::notify`; used by `shutdown_webserver` / `shutdown_webserver_tcp` |

The two uid lanes of DPD records (`34626/dpd` vs `dhamma-1-01/dpd`) are
explained in `CLAUDE.md`, "DPD records correlate to dict_words".

## Shared helpers (`api/src/helpers.rs`)

These are `pub` in `simsapa_api`, so the CLI's `search` command calls the same
`run_search` as `POST /search`; that is what makes a command's `--json` output
the route's body.

- `parse_search_mode` / `parse_search_area`: request string → enum (exact serde
  labels), `None` on unknown (→ 400 on `/search`).
- `build_search_params(request, mode, area)`: builds the `SearchParams` literal:
  area-aware language/source filters, `page_len` (default 20),
  `show_all_snippets` / `snippet_exclude` from the request, defaults for the rest.
- `run_search(dbm, query_text, params, area, page_num, deconstructor)`: lazy
  mode-gated searcher init, then `SearchQueryTask` + `results_page` +
  `total_hits`, returning `ApiSearchResult`; logs and returns empty on error.
- `run_suttas_search(request, dbm, fallback_mode)`: shared body for the two
  named Suttas routes: reference → `UidMatch` auto-detect, else `fallback_mode`.
- `run_search_with_uid_fallback(...)`: wraps `run_search` with the
  self-correcting 0-hit → `fallback_mode` re-run of the Suttas routes.
- `run_dict_combined_with_fallback(...)`: the dictionary self-correcting chain
  (0-hit auto-`UidMatch` → normalized `UidMatch` → `DpdLookup`), used by
  `/dict_combined_search` and `/search`'s Dictionary `Combined` path.
  `SearchQueryTask` rejects `Combined + Dictionary`, which is why `/search`
  remaps it.
- `word_json_response` / `word_html_response` / `sutta_html_response`: shared
  bodies for the retrieval routes (resolver + 404-on-miss + the `?verbose=1`
  envelope).

## Lazy, mode-gated fulltext searcher init

The GUI's webserver runs on a thread in the **same process** as the GUI
(`cpp/gui.cpp` spawns `start_webserver`), so it shares the one process-global
`FULLTEXT_SEARCHER` (`backend/src/lib.rs`). The query path does **not**
self-init the searcher: `with_fulltext_searcher(...)` returns `None` when
uninitialized and the query returns **silent-empty** results.

`run_search` therefore calls `simsapa_backend::init_fulltext_searcher()`
**only** when the resolved mode needs the Tantivy index (`FulltextMatch` or
`Combined`), immediately before the query. It is idempotent, so in steady state
it does nothing; it does real work only when QML init never ran (QML's
`load_searcher` normally opens it).

Init is **not** eager at `start_webserver()`: right after the webserver thread
is spawned, `gui.cpp` runs `reconcile_dict_indexes_blocking_c()`, which does
Tantivy **writes** and then `reinit_fulltext_searcher()`. Opening a reader
eagerly on the API thread would contend with those writes and pay the cold
index-open cost even for clients that never query fulltext. Searcher access is
behind an `RwLock`; the API and QML threads are concurrent readers.

`simsapa serve` has the same lazy init. It runs its startup preamble (including
any dictionary-index reconciliation) to completion before it listens, so there
are no concurrent index writes to avoid, but the cold index-open cost is still
paid only by the first fulltext query.
