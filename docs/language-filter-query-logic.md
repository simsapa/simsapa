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
  It is reached through `SearchBarInput.ensure_dropdowns_restored()` on
  `Component.onCompleted` and on `onSearch_areaChanged`. On `onIs_wideChanged`
  the model is rebuilt keeping the dropdown's **own** current key instead. A
  `suppress_persist` flag + `applied_area` guard prevent programmatic restores
  and mid-transition model changes from persisting or firing a query —
  identical to `search_mode_dropdown`.

#### What a query reads: `mode_for_query()` / `language_for_query()`

`get_search_params_from_ui()` (in `SuttaSearchWindow.qml`) takes the mode from
`search_mode_dropdown.mode_for_query()` and the language from
`language_filter_dropdown.language_for_query()`. Never read either dropdown's
`get_text()` or the saved setting directly. Both functions follow one rule:

- **Before the dropdown is restored for the current area**
  (`applied_area !== search_area`): the saved per-area value, i.e. what the
  restore is about to show. A fallback — the entry points below restore first.
- **After:** the dropdown's own selection (`get_text()`).

Three traps make that the rule:

1. **Handler order is unspecified.** On an area switch the query and the two
   restores are separate `onSearch_areaChanged` handlers; on initial load the
   query is in root's `Component.onCompleted` and the restores in the
   dropdowns'. Measured offscreen (Qt 6.9.3, Fusion), the query runs **first**
   in both cases. At that moment `currentIndex` holds what the model change left
   behind — index 0 of the new area's modes (`QQuickComboBox::setModel` resets
   it on the spot) and the *previous* area's language — so `get_text()` would
   run a Fulltext query while "Contains Match" is shown a moment later. Hence
   `ensure_dropdowns_restored()`, called by **every** entry point (both
   dropdowns' handlers, the coordinator, root's `onCompleted`) before anything
   else: whichever runs first restores, the rest find `applied_area` current and
   skip. Skipping also avoids a second distinct-values query for the language
   labels.
2. **The saved values are process-global.** `get_last_search_mode` /
   `get_language_filter_key` read the one `AppData` settings cache that every
   open search window shares. Read for a restored dropdown, window A's query
   would use window B's latest choice while A's dropdown still shows its own.
   The same reason keeps two other places on the dropdown's own state: the
   language `onCurrentIndexChanged` **no-op guard** compares the new key with
   `applied_key` (this dropdown's last applied key), not with the saved key —
   against the saved key, picking in A the key B has just saved would persist
   nothing and run no query — and a width relabel keeps the current key rather
   than re-reading the saved one. A deliberate area switch *does* adopt the saved value: that is the
   "last used mode/language" behaviour.
3. **A model assignment resets `currentIndex` to 0 immediately**, so it must sit
   inside `suppress_persist` together with the index restore. Outside it, a
   width relabel saves the reset as `"Language"` and fires an unfiltered query
   while the dropdown goes on showing the old key (a phone rotation crosses
   `is_wide`). For the same
   reason the mode dropdown's `model` is assigned in `restore_for_current_area()`
   rather than bound to `root.search_area`: a binding re-evaluating after the
   restore would wipe the restored index, with `applied_area` already current.

### Exactly one query per area switch

Both dropdowns' `restore_for_current_area()` are **pure** — they restore the
saved mode/language but never fire a query. The single query for an area switch
is fired by `area_query_coordinator`, a `Connections { target: root }` in
`SearchBarInput.qml`, after `ensure_dropdowns_restored()`. The dropdowns'
`onCurrentIndexChanged` handlers ignore the index changes of a switch
(`suppress_persist` during a restore, `applied_area` before it), and nothing
depends on the coordinator running before or after the dropdowns' handlers. The
one initial query is fired from `root.Component.onCompleted`, likewise after
`ensure_dropdowns_restored()`.

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
