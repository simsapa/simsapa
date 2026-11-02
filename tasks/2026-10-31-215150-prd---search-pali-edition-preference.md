# PRD: Search: Pāli edition preference

## 1. Introduction / Overview

Simsapa holds two Pāli editions of many texts: Mahāsaṅgīti (`/pli/ms`) and CST (`/pli/cst`).
`include_cst_mula_in_search_results` defaults to false, so that a sutta does not appear twice
in search results. That also hides the CST-only texts, such as the Vinaya divisions.

This feature lets the user choose which edition to see of a text that is in both, and keeps
every text that is in only one edition findable.

Split off from the [OpenTipitaka PRD](./2026-10-31-192653-prd---cst-line-by-line-text-and-opentipitaka-english.md). It ships with the same new
database version.

## 2. Goals

1. Stop a text that exists in both Pāli editions from appearing twice in search results,
   while keeping every text findable.

## 3. User Stories

- As a user searching in Pāli, I want to choose which edition I see of a text that is in
  both, and I want the texts that are in only one edition to appear whatever I choose.

## 4. Functional Requirements

1. `include_cst_mula_in_search_results` must default to true, so that CST-only texts are
    found. Existing users must get this too, through the `AppSettings` deserialize hook,
    because a stored `false` is the old default and not a choice. A choice made after the
    migration is kept.
2. Add the setting `pali_edition_preference`, with the values `ms`, `cst` and `both`. The
    default is `ms`, which keeps today's results for texts in both editions.
3. A record is **in both editions** when it is an MS Pāli record or a mapped CST mūla record
    and `sutta_refs_are_related` holds between it and a record of the other edition. So
    `an5.303/pli/ms` and `an5.303-1152/pli/cst` both count. CST `.xml` records and
    commentaries never count, because they have no comparable reference.
4. Preference `ms` hides the CST records that are in both editions, and preference `cst`
    hides the MS records that are in both. `both` hides nothing. A record in only one edition
    is never hidden, so Vinaya divisions stay findable under `ms`, and the Theragāthā
    records stay findable under `cst`.
5. The preference applies only when both editions are included. The existing "MS Mūla in
    Search" and "CST Mūla in Search" checkboxes keep their meaning, and exclude a whole
    edition.
6. The records in both editions must be computed once, at CST import, and stored in a new
    appdata table, `pali_edition_twins`, holding their uids. A table keeps `Sutta` unchanged,
    so a failed migration, which is non-fatal, cannot break every sutta read. The SQL
    filters read it, and the Tantivy indexer writes it into a new `has_other_edition` field,
    so there is one definition.
7. The filter must apply in ContainsMatch, FulltextMatch, Title Match and Combined, in the GUI,
   the API and the CLI. It must **not** apply to Uid Match, which names a record: a search for
   `mn1/pli/cst` must find what it names under any preference. A bare reference, such as
   `mn1`, names a text and not a record, so the preference applies to it. All four uid paths
   call `filter_suttas_query`, so the edition filter must be separate from that function.
8. The search API and the CLI must default to `ms`, like the GUI, with an option to select
   `cst` or `both`:
   - the API takes a `pali_edition` field in the search request, on every route that searches
     suttas, and answers HTTP 400 to an unknown value, as it does for an unknown mode;
   - the CLI takes `--pali-edition ms|cst|both` on `simsapa search` and
     `simsapa fulltext-search`.

   They do not read the GUI setting, so the same request always gets the same answer.
9. The advanced search options must offer the preference beside the two checkboxes, enabled
   only when both are checked.
10. A missing `pali_edition_twins` table or `has_other_edition` field must not break search,
    because the filter is on by default everywhere. A migration failure is non-fatal, and an
    index built by an older version does not have the field. In these cases the filter is
    skipped, so results show both editions, and this is logged once. Whether the table exists
    is checked once per process, never from error text.

- As an agent using the API or the CLI, I want one result per text by default, an option for
  the other edition, and every record reachable by its uid.

## 5. Non-Goals (Out of Scope)

- Any filter or setting for `opentipitaka` English. FR-37 of the OpenTipitaka PRD stands: the
  CST mūla filter stays Pāli-only.
- Applying the preference to the translation lists in the reader.

## 6. Design Considerations

The preference sits in the advanced search options, after the "CST Mūla in Search" checkbox.

## 7. Technical Considerations

- The `pali_edition_twins` table needs one new appdata migration. The new Tantivy field needs
  an `INDEX_VERSION` change.
- The default changes what API and CLI clients get today: a search no longer returns the
  `pli/cst` record of a text that is in both editions. The docs must say so.

## 8. Success Metrics

1. With the default settings, a phrase in `mn1` returns the `pli/ms` record and not the
   `pli/cst` one, and a phrase in a Vinaya division returns its `pli/cst` record.
2. An existing user's stored `include_cst_mula_in_search_results: false` becomes true once.
3. `POST /search` and `simsapa search` return the `pli/ms` record of `mn1` by default, the
   `pli/cst` record with `cst`, and both with `both`. An unknown `pali_edition` is HTTP 400.
4. A Uid Match for `mn1/pli/cst` returns that record under every preference.
5. With the table or the field missing, search still returns results.
