# Language Filter Query Logic

## Overview

The search bar has a **language filter dropdown** (`language_filter_dropdown` in
`bridges/assets/qml/SearchBarInput.qml`) that restricts results to a single language. It
applies to all three search areas — **Suttas**, **Dictionary**, and
**Library** — and its selection is **persisted separately per area**, the same
way the search mode is.

The dropdown's first entry is the sentinel `"Language"` (or `"Lang"` on narrow
screens), which means **no language filter**. Any other entry is a concrete
language code (e.g. `"pli"`, `"en"`) drawn from the distinct language values in
the database.

## The "no filter" sentinel

`"Language"` is a fixed keyword meaning *"no language filter is selected"*. It is
always index 0 and is the default for every area (the UI never auto-selects a
concrete language).

- **QML:** `language_filter_dropdown.get_text()` returns `"Language"` for index 0
  and the language code otherwise. This value is placed into the search params as
  `params.lang`.
- **Backend gate:** Every query path applies the filter only when

  ```rust
  !self.lang.is_empty() && self.lang != "Language"
  ```

  So when nothing is selected (empty string or the `"Language"` sentinel) **no
  filter clause is added at all** — an unfiltered search is therefore no slower
  than before the feature existed. This gate is the single source of truth; do
  not introduce a different default.

## Per-area persistence

The selected language key is stored per search area, mirroring the search-mode
persistence (`search_last_mode`):

- **Storage:** `AppSettings.search_last_language: IndexMap<String, String>`
  (`backend/src/app_settings.rs`), keyed by area name (`"Suttas"`,
  `"Dictionary"`, `"Library"`). A missing entry, an empty value, or the
  `"Language"` sentinel all mean "no filter".
- **Accessors:** `AppData::get_language_filter_key(area)` /
  `set_language_filter_key(area, key)` (`backend/src/app_data.rs`). The setter
  updates the in-memory cache synchronously and persists to disk off the UI
  thread, exactly like `set_last_search_mode`.
- **Bridge:** `SuttaBridge::get_language_filter_key(area)` /
  `set_language_filter_key(area, key)` (`bridges/src/sutta_bridge.rs`).
- **QML:** `language_filter_dropdown.restore_for_current_area()` rebuilds the
  model for the current area and restores the saved key (defaulting to index 0).
  It runs on `Component.onCompleted`, on `onSearch_areaChanged`, and on
  `onIs_wideChanged`. A `suppress_persist` flag + `applied_area` guard prevent
  programmatic restores and mid-transition model rebinds from persisting or
  firing a query — identical to `search_mode_dropdown`.

#### The persisted key is the source of truth for the query

`get_search_params_from_ui()` (in `SuttaSearchWindow.qml`) reads the language
from `SuttaBridge.get_language_filter_key(search_area)` — **not** from the
ComboBox's `currentIndex` / `get_text()`. This is deliberate:

> On an area switch the language dropdown's `model` is reassigned imperatively
> (`load_language_labels_for_area`). Qt **defers** the ComboBox's `count` /
> `currentIndex` reconciliation, so `currentIndex = idx` is briefly clamped
> against the stale count and `get_text()` can return the *previous* area's
> value at the exact moment the area-switch query fires ("one step behind").

The persisted key is updated synchronously in the in-memory settings cache on
every user change (`onCurrentIndexChanged`) and re-applied to the dropdown on
every area/width change, so it is always correct for the current area regardless
of ComboBox timing. The `onCurrentIndexChanged` handler also has a **no-op
guard**: if the new value already equals the persisted key (e.g. a deferred
reconciliation re-asserting the restored index), it skips persisting and skips
the query, so the area switch still fires exactly one query (from the
coordinator).

### Exactly one query per area switch

Both dropdowns' `restore_for_current_area()` are **pure** — they restore the
saved mode/language but never fire a query. The single query for an area switch
is fired by `area_query_coordinator`, a `Connections { target: root }` declared
**after both dropdowns** in `SearchBarInput.qml`. Because QML connects signal
handlers in creation order, the coordinator connects last and therefore runs
*after* the ComboBox `model` bindings have re-evaluated and after both restores
— so the one query reads the freshly restored mode + language and never fires
twice. (A second query on every area switch would waste real compute.)

> A root *inline* `onSearch_areaChanged` would connect **before** the child
> dropdowns' `model` bindings and fire too early (against a stale model),
> producing a wrong-mode query plus a second corrective query. The coordinator
> must be a `Connections` object placed after the dropdowns. The one initial
> query is fired from `root.Component.onCompleted`, which runs after the child
> dropdowns have restored.

> The pre-existing single `sutta_language_filter_key` string was removed in
> favour of the per-area map. Old persisted settings simply lose that field on
> deserialization (serde ignores unknown fields); the new map defaults to empty.

## Distinct-value loading (the dropdown options)

The selectable languages are the **distinct language values present in the
database**, queried on demand when the dropdown model is built
(`SearchBarInput.qml::load_language_labels_for_area`). The same on-demand
approach is used for every area for consistency — there is no separate startup
cache for languages:

| Area       | Bridge method                    | Backend source                                                        |
| ---------- | -------------------------------- | --------------------------------------------------------------------- |
| Suttas     | `get_sutta_language_labels()`    | `DbManager::get_sutta_languages()` → `suttas.language` SELECT DISTINCT |
| Library    | `get_library_language_labels()`  | `indexer::get_library_languages()` → spine/book effective language    |
| Dictionary | `get_dict_language_labels()`     | `DictionariesDbHandle::get_distinct_languages()` → `dict_words.language` |

All three return exactly the distinct values present in the database, with **no
fallback default**. The Dictionary area used to inject a hardcoded `"pli"` when
the query returned nothing, but that was removed: the built-in dictionaries
include `"en"` sources (e.g. DPPN), so a hardcoded `"pli"` default is wrong, and
the other areas add no such fallback. If there are no values, the dropdown shows
only the `"Language"` (no filter) sentinel — consistent across areas.

## Where the filter is applied in queries

`params.lang` flows into `SearchQueryTask` as `self.lang`. The filter is applied
in `backend/src/query_task.rs` (and the fulltext searcher), always behind the
gate above:

### Suttas

- `suttas_contains_match_fts5` — `AND f.language = ?` in the FTS5 SQL.
- `uid_sutta_all` / `uid_sutta_range_all` — `.filter(language.eq(&self.lang))`.
- `fulltext` searcher — via `SearchFilters.lang` + `lang_include`.

### Library

- `book_spine_items_contains_match_fts5` — `AND f.language = ?` in the SQL.
- `fulltext_library` — via `SearchFilters.lang`.

### Dictionary

All dictionary modes honor the filter:

- **ContainsMatch** — `dict_words_contains_match_fts5_full` pushes the filter
  into **all four phases**: Phases 1, 2, 4 (DPD-headword-driven) add
  `.filter(dict_dsl::language.eq(self.lang.clone()))` on the resolved
  `dict_words` row; Phase 3 (unified `dict_words_fts` retrieval) appends
  `AND d.language = ?` to the raw SQL.
- **FulltextMatch** — `fulltext_dict` via `SearchFilters.lang`.
- **HeadwordMatch** — `lemma_1_dpd_headword_match_fts5_full` resolves to
  `dict_words` on both paths, so the same `dict_words.language` filter applies:
  Path A (DPD, `dict_label = "dpd"`) is excluded under a non-Pāli filter, while
  Path B keeps non-DPD headword matches in the selected language.
- **DpdLookup** — `dpd_lookup_full` is a *pure DPD* path (queries the DPD DB
  directly, not `dict_words`), so it cannot filter on a `language` column.
  Instead it short-circuits via `dpd_excluded_by_lang()`: since every DPD
  headword is Pāli, a non-Pāli filter returns an empty result set.
- **Combined** (bridge-orchestrated) — fans out a `DpdLookup` sub-query + a
  `FulltextMatch` sub-query (`bridges/src/sutta_bridge.rs::fetch_combined_page`).
  Both sub-queries honor the filter via the mechanisms above, so Combined is
  correct transitively (under `"en"`, the DPD side returns nothing and the
  Fulltext side returns only `"en"` dict rows).
- **Bold commentary definitions** (`include_comm_bold_definitions`) appended by
  the `*_with_bold` variants are DPD-derived Pāli text, so they are also gated by
  `dpd_excluded_by_lang()` in `query_bold_definitions_bold_fts5` /
  `query_bold_definitions_commentary_fts5`.

> **Gotcha — DPD entries are `language = "pli"`.** DPD headwords are Pāli words
> with English definitions, and they are stored with `language = "pli"`. So a
> `"pli"` filter *includes* DPD while an `"en"` filter *excludes* it, even though
> the definition text is English. This is intentional: the language column
> describes the headword, not the definition body. `dpd_excluded_by_lang()`
> centralises the "DPD/bold are Pāli-only" rule for the paths that can't filter
> on a `dict_words.language` column. The behaviour is locked in by
> `test_dict_word_search_contains_match_with_language_filter`,
> `test_dict_word_dpd_lookup_with_language_filter`, and
> `test_dict_word_headword_match_with_language_filter` in
> `backend/tests/test_query_task.rs`.

## Adding a new search area or query path

When you add a query path that should honor the language filter:

1. Read `self.lang` and gate with `!self.lang.is_empty() && self.lang != "Language"`.
2. Apply the filter only inside that gate, so the unfiltered path stays cost-free.
3. If the area has its own dropdown options, add a `get_*_language_labels()`
   bridge method backed by a distinct-value query, and wire it into
   `load_language_labels_for_area`.
4. Persistence is automatic — `restore_for_current_area()` and
   `get/set_language_filter_key(area)` are area-agnostic.
