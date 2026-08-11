# Tasks — Topic Index list readability and paragraph links

Source PRD: [2026-08-11-144238-prd---topic-index-list-readability-and-paragraph-links.md](./2026-08-11-144238-prd---topic-index-list-readability-and-paragraph-links.md)

Date: 2026-08-11

## Component Analysis

Technical components the PRD requires, and what blocks what.

| # | Component | Layer | Depends on | PRD requirements |
|---|---|---|---|---|
| C1 | Suffix pre-computation in the CIPS parser | CLI (Rust) | — | 9–12, 48, 58 |
| C2 | Anchor validation + warning summary | CLI (Rust) | segment-key lookup closure (C3) | 50–57 |
| C3 | Segment-key lookup plumbed into `parse_cips_index()` | CLI (Rust) | — | 51, 55, 56, 57 |
| C4 | Sort/allocation cleanups in the parser | CLI (Rust) | — | §6.1 |
| C5 | Regenerated `assets/general-index.json` | Build artefact | C1, C4 | 48, success metric 9 |
| C6 | `suffix` field on backend `TopicIndexRef` | Rust backend | — (parses old JSON too) | 49, §6.10 |
| C7 | Sub-topic colon + vertical rhythm | QML | — | 1–5 |
| C8 | Reference label without segment id, suffix appended | QML | C6 (for the field) | 6–8, 10, 11, 13 |
| C9 | `open_sutta()` emits `anchor`, drops `segment_id` | QML | — | 14–16, 37, 38 |
| C10 | Already-open-sutta re-scroll | QML (`SuttaSearchWindow`) | C9 | 17, §6.15 |
| C11 | Anchor resolution + fallback walk in the page | TypeScript + webview wrappers | C9 | 19, 22–24, §6.3 |
| C12 | Paragraph highlight class + Sass | Sass / injected JS | C11 | 18, §6.5 |
| C13 | In-page notice component (fallback + give-up forms) | TypeScript + Sass | C11 | 20, 21, 25–36 |
| C14 | `show_references` on `SuttaDisplayDefaults` | Rust backend | — | 40, §6.17 |
| C15 | `show_references: Option<bool>` precedence resolution | Rust backend + routes | C14 | 43, 44, 45 |
| C16 | "Show references" segmented control in the panel | HTML template + TS | C15 | 39, 41, 42, 46 |
| C17 | Persist path through `POST /save_sutta_display_settings` | TS + Rust | C14, C16 | 41 |
| C18 | Docs update | docs/ | C15, C16 | §6.17 |
| C19 | Tests (Rust unit, Jest, QML lint) | all | all | 58, success metric 13 |

Dependency order: **C3 → C2**; **C1/C4 → C5 → (visible effect of) C8**; **C6 → C8**;
**C9 → C10/C11 → C12/C13**; **C14 → C15 → C16 → C17**. C1–C5 and C14–C17 are
independent of each other and of C7–C13, so the stages below can land in the
order given without leaving the tree broken.

### Requirement coverage check

Every numbered functional requirement maps to at least one component:
1–5 → C7 · 6–8 → C8 · 9–12 → C1 (+C8 render) · 13 → C7/C8 · 14–16 → C9 ·
17 → C10 · 18 → C12 · 19 → C11 · 20–21 → C13 · 22 → C11 · 23–24 → C11 ·
25–36 → C13 · 37–38 → C9 · 39 → C16 · 40 → C14 · 41 → C17 · 42 → C16 ·
43 → C15 · 44 → C15/C11 · 45 → C15/C16 · 46 → C16 · 47 → C12 (optional) ·
48 → C1 · 49 → C6 · 50–57 → C2/C3 · 58 → C19.

## Assessment of the Current State

Verified in the tree today (not taken from the PRD):

- `TopicIndexWindow.qml:203` writes `segment_id` into the result JSON.
  `SuttaSearchWindow.qml:367` (`new_tab_data`) and `:1228` (existing-tab update)
  read `anchor` and nothing reads `segment_id` — this is the whole of the
  "clicking a link ignores the location" bug.
- `scroll_to_anchor()` is duplicated verbatim in `SuttaHtmlView_Desktop.qml:196`
  and `SuttaHtmlView_Mobile.qml:306`, including the third branch
  `document.querySelector('${root.anchor}')`, which throws `SyntaxError` on a
  colon-bearing id and aborts the IIFE. Both are driven by a `scroll_timer`
  restarted from `onLoadingChanged` on `LoadSucceededStatus`
  (`_Desktop.qml:41/335`, `_Mobile.qml:62/443`).
- Both wrappers already build `?anchor=…#…` URLs
  (`_Desktop.qml:137-142`, `_Mobile.qml:247-251`). **The anchor is therefore
  part of the URL**, and `onData_jsonChanged` (`_Desktop.qml:241`) calls
  `load_sutta_uid()` on every change — so a *different* anchor on an
  already-open sutta produces a different URL, a real reload, and the existing
  `scroll_timer`. Requirement 17 is already satisfied for that case; only "same
  uid **and** same anchor" is inert. See task 5.4.
- The callback form `runJavaScript(script, fn)` is already used unconditionally
  on both platforms (`SuttaSearchWindow.qml:147`,
  `get_current_scroll_position()`), so task 6.10/6.11 needs no platform branch
  for the logging (open question 3 is narrower than stated).
- `WindowManager::open_sutta_search_window_with_query()`
  (`cpp/window_manager.cpp:497`) invokes the **same**
  `show_result_in_html_view_with_json` on the fresh window's root, so the
  new-window mode needs no anchor plumbing (task 5.3 is a confirmation).
- The segment wrapper's `id` is emitted in **both** render paths regardless of
  `show_references` (`helpers.rs:2313` segmented, `:2437` multi-column), so
  requirement 44 holds structurally — but the `id` is emitted only for keys that
  have a `content_json_tmpl` entry (`helpers.rs:~2316`; measured: 0 untemplated
  keys across the 32 referenced suttas).
- `SuttaDisplayDefaults` already carries a **struct-level** `#[serde(default)]`
  (`app_settings.rs:468`), and `POST /save_sutta_display_settings`
  (`api.rs:1717`) deserializes straight into it while the client posts its whole
  settings object — so the persist path (C17) costs nothing.
- `TopicIndexRef`'s optional fields have no `#[serde(default)]` and deserialize
  from JSON that omits them today, so the new `suffix` field will too (§6.10's
  independence claim, confirmed).
- `api.rs:731` — `let show_references = anchor.is_some();`, passed to
  `try_render_sutta_html_by_uid_with_overrides` at `:741`.
  `resolve_sutta_display_options(&self, sutta, show_references: bool, overrides)`
  at `app_data.rs:385`, forwarded to `SuttaDisplayOptions::resolve`
  (`sutta_display.rs:41-49`). Only `/sutta_content_block` (`api.rs:1651`) accepts
  a `show_references` query parameter; the two full-page routes (`:779`, `:1613`)
  do not.
- `SuttaDisplayOverrides` (`sutta_display.rs:16-21`) holds `layout`, `columns`,
  `repeat_pali` — adding `show_references: Option<bool>` here is the consistent
  shape and removes the awkward positional `bool`.
- The two `TopicIndexRef` structs (`cli/src/bootstrap/parse_cips_index.rs:25`,
  `backend/src/topic_index.rs:20`) are field-identical; both already use
  `skip_serializing_if = "Option::is_none"` on the optional fields, so the new
  `suffix` field follows an existing pattern exactly.
- The Topic Index delegate is `TopicIndexWindow.qml:448-522`: a `ColumnLayout`
  per sub-topic with `spacing: 2` and `Layout.leftMargin: 20`, whose links sit in
  a `Flow` with `Layout.leftMargin: 10`.
- `show_references` already round-trips through the client exactly as the PRD
  says: `content_reload.ts:17/26` (URL), `:130` (write-back), `:223`
  (`refetch_with_params` reads `SUTTA_DISPLAY.show_references`). The TS
  `SuttaDisplaySettings` interface (`display_settings.ts:24-34`) does **not**
  have the field — that is the missing piece on the client.
- `display_settings.ts` rerender handler is `(layout, repeat_pali) => void`
  (`:69`, `:73`, wired in `simsapa.ts:170-172`) — it must widen to carry
  `show_references`.
- Sass partials are pulled into the sutta stylesheet with
  `@include meta.load-css("find")` in `assets/sass/suttas.sass:119`; the new
  partial is added the same way.
- The regeneration command already exists as `make parse-cips`
  (`Makefile:67`) and passes `--minify`, so the committed JSON is one line —
  diffing it needs a pretty-printed pass on both sides.

Relevant existing docs to read before starting:
`docs/sutta-display-settings-and-multi-column-view.md` (the CSS-on-cells rule
and the post-swap re-init contract) and
`docs/search-snippet-highlight-pipeline.md` §snippet-aware find-bar jump (the
"same sutta already open" precedent for task 5).

## Relevant Files

- `cli/src/bootstrap/parse_cips_index.rs` — CIPS parser: suffix pre-computation, anchor validation, sort cleanups, unit tests.
- `cli/src/main.rs` — `parse_cips_index_command()` (`:770-830`); the pre-loaded title map to extend with a segment-key lookup.
- `assets/general-index.json` — the generated index; regenerated with the new `suffix` fields.
- `backend/src/topic_index.rs` — `TopicIndexRef`; add the optional `suffix` field.
- `assets/qml/TopicIndexWindow.qml` — layout rhythm, label formatting, `open_sutta()` anchor.
- `assets/qml/SuttaSearchWindow.qml` — already-open-sutta re-scroll (`show_result_in_html_view()` `:1095`, existing-tab update `:1228`).
- `assets/qml/SuttaHtmlView_Desktop.qml` — `scroll_to_anchor()`, anchor URL construction.
- `assets/qml/SuttaHtmlView_Mobile.qml` — the mobile twin of the above.
- `src-ts/anchor_jump.ts` (new) — candidate walk, highlight, notice; exposed on `window` for the wrappers to call.
- `src-ts/anchor_jump.test.ts` (new) — unit tests for the walk and the notice rules.
- `src-ts/simsapa.ts` — init wiring for the above and for the widened rerender handler.
- `assets/sass/_anchor_jump.scss` (new) — highlight + notice styling.
- `assets/sass/suttas.sass` — `@include meta.load-css("anchor_jump")`.
- `assets/sass/_find.scss` — reference for the existing light/dark highlight pair (`:242-258`).
- `backend/src/app_settings.rs` — `SuttaDisplayDefaults` (`:469`) and its `Default` impl (`:485`).
- `backend/src/sutta_display.rs` — `SuttaDisplayOverrides` / `SuttaDisplayOptions::resolve` / the GET-param parser.
- `backend/src/app_data.rs` — `resolve_sutta_display_options()` (`:385`), `sutta_display_js()` (`:646-655`).
- `bridges/src/api.rs` — `sutta_html_response()` (`:719`), routes `:779` / `:1613`, `/sutta_content_block` (`:1650`), `POST /save_sutta_display_settings` (`:1716`).
- `assets/templates/display_settings.html` — the Layout `ds-section` (`:25-51`).
- `src-ts/display_settings.ts` + `src-ts/display_settings.test.ts` — panel state, scope semantics, save payload.
- `src-ts/content_reload.ts` + `src-ts/content_reload.test.ts` — `build_content_block_url()` (`:17-27`), post-swap write-back (`:130`), `refetch_with_params()` (`:218-224`).
- `docs/sutta-display-settings-and-multi-column-view.md` — must gain the new option, its scope semantics and the precedence rules.
- `PROJECT_MAP.md` — updated for the new files.

### Notes

- `make sass` after editing `assets/sass/`; never hand-edit `assets/css/`.
- `npx webpack` (or `make simsapa.min.js`) after editing `src-ts/`; the page
  loads `assets/js/simsapa.min.js`.
- A new **QML** component file must be added to `qml_files` in `bridges/build.rs`
  in the exact `"../assets/qml/<Name>.qml"` form. (No new QML component is
  planned below — the new files are TypeScript and Sass.)
- QML logging goes through `Logger { id: logger }` with a **single** concatenated
  string argument — never `console.*`, never comma-separated arguments.
- Tests: `make rust-test` (backend + cli), `make qml-test`, `make js-test`, or
  `make test` for all three.
- Do not run the GUI for verification; the device/visual checks in §8 of the PRD
  are for the user to perform. Task 8 ends by handing that checklist over.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown
file by changing `- [ ]` to `- [x]`. Update the file after completing each
sub-task, not just after completing an entire parent task.

---

## Tasks

### Specs for 1.0 — suffix pre-computation

**Data contract.** New optional field on the CLI's `TopicIndexRef`:

```rust
/// Disambiguation letter when two refs in the same entry share a displayed
/// label ("a", "b", … "aa"). Absent when the label is unique.
#[serde(skip_serializing_if = "Option::is_none")]
pub suffix: Option<String>,
```

**The collision key must be the string QML will display**, or the suffixes will
appear on labels that do not actually look alike. That string is
`format_sutta_ref(sutta_ref_without_segment) + " " + title` — i.e. what task 4.5
produces. Implement one helper in the parser that computes it and note in a
comment that it mirrors `TopicIndexWindow.qml`'s `format_sutta_ref()`.

**Scope** is one `refs` array (one sub-topic entry), `type == "sutta"` only
(requirements 12, 13).

**Dependencies:** none. This group must not change the generated JSON's sort
order — that is verified in 3.0.

- [x] 1.0 CLI `parse-cips-index`: pre-compute disambiguation suffixes and clean up the sorts
  - [x] 1.1 Add the `suffix: Option<String>` field to `TopicIndexRef` in `cli/src/bootstrap/parse_cips_index.rs:25`, with the doc comment and `skip_serializing_if` shown above, and set it to `None` at the existing construction sites so the file compiles.
  - [x] 1.2 Add `fn display_label(sutta_ref: &str, title: Option<&str>) -> String` — take the part before `:`, upper-case the collection letters and insert a space (mirroring the QML `format_sutta_ref()` regex `^([a-z]+)(\d.*)$`), then append `" " + title` when a title is present. Unit-test it against `dn33:1.11.0` → `DN 33 Saṅgītisutta` and `sn35.24` → `SN 35.24 Pahānasutta`.
  - [x] 1.3 Add `fn suffix_letter(n: usize) -> String` producing `a`…`z`, then `aa`, `ab`, … Keep it a plain base-26 continuation as the PRD allows; unit-test `0 → "a"`, `25 → "z"`, `26 → "aa"`.
  - [x] 1.4 Add `fn assign_disambiguation_suffixes(refs: &mut [TopicIndexRef])`: group the `type == "sutta"` refs by `display_label`, and for every group with **two or more** members assign `suffix = Some(suffix_letter(i))` in data order. Groups of one keep `None` (requirement 11). Cross-references are skipped entirely (requirement 13).
  - [x] 1.5 Call `assign_disambiguation_suffixes(&mut refs)` in `IndexBuilder::build()` (`:441-470`) after the sutta refs **and** the xrefs have been pushed for a sub-entry, just before the `TopicIndexEntry` is constructed — so the scoping is per entry (requirement 12) and never across entries or headwords.
  - [x] 1.6 Replace `headword_keys.sort_by_key(|a| get_headword_sort_key(a))` (`:419`) with `sort_by_cached_key`. Verify the resulting order is identical (the key function is pure, so it must be — but see 3.2).
  - [x] 1.7 Replace the `sub_keys.sort_by(…)` closure (`:434-442`) with `sort_by_cached_key` over a tuple key that preserves the em-dash-first rule, e.g. `(sub != "—", latinize(sub).to_lowercase())` — `false` sorts before `true`, so `"—"` stays first. Do not change the comparison semantics.
  - [x] 1.8 Replace `sorted_locators.sort_by(|a, b| compare_locators(a, b))` (`:446`) with `sort_by_cached_key` over `(book_order_index(&extract_book(l)), extract_numbers(l))`. **Equivalence has been checked** (`compare_locators`, `:205-231`): its zipped element-wise loop plus the final `nums_a.len().cmp(&nums_b.len())` is exactly `Vec<u32>`'s derived lexicographic `Ord`, and both sorts are stable — the tuple key reproduces the current order exactly, with no tie-break left over. Task 3.2's diff remains the guard.
  - [x] 1.9 Replace the `format!("{} ", word)` allocation in `get_headword_sort_key`'s strip loop (`:163`) with a `const IGNORE_PREFIXES: &[&str]` of already-spaced prefixes plus `strip_prefix`. Keep the repeated-strip behaviour and the leading-quote trim.
  - [x] 1.10 Change `validate_index`'s `all_headwords: HashMap<String, bool>` (`:511`) to a `HashSet<String>` and its `contains_key` to `contains`.
  - [x] 1.11 Add unit tests in the existing `mod tests` for `assign_disambiguation_suffixes` covering the four PRD cases (requirement 58): no collision → no field; two colliding → `a`/`b`; a five-way group → `a`–`e`; two refs differing only by title → no suffix. Add a fifth: a group containing an xref is unaffected.
  - [x] 1.12 Run `cd cli && cargo test` and confirm the whole existing suite still passes.

---

### Specs for 2.0 — anchor validation

**Signature contract.** `parse_cips_index()` / `parse_cips_to_json()` currently
take one `title_lookup: F where F: Fn(&str) -> Option<String>`. Add a **second,
optional** closure so the parser stays testable without a database and
requirement 57's skip path is a `None` rather than a special case:

```rust
/// What the parser learns about a referenced sutta's segments.
pub enum SuttaSegments {
    /// No `{uid}/pli/ms` row exists (bad reference in the CSV).
    UnresolvedUid,
    /// The sutta exists but its content_json is empty (a legacy text).
    NoSegments,
    /// The segment keys the sutta's content actually has.
    Keys(HashSet<String>),
}

segments_lookup: Option<&dyn Fn(&str) -> SuttaSegments>
```

`None` = validation skipped entirely (requirement 57).

**Uid resolution must match the runtime**: `{uid}/pli/ms` first, as
`AppData::get_full_sutta_uid()` does (`backend/src/db/appdata.rs:258`) —
requirement 51.

**Output contract** (requirement 53), printed after the existing validation
warnings in `parse_cips_to_json()` (`:604-611`):

```
Anchor validation: 1579 checked, 1573 ok, 0 unresolved uid, 0 no segments, 6 missing segment
  missing segment: 'conditions (saṅkāra)' / 'all beings sustained by' -> dn33:1.7.9.1
    ^ headword spelling is a CSV typo (saṅkhāra); reproduce it verbatim, do not "fix" it here
  …
```

A clean run prints one line, not nothing.

**Dependencies:** the title map already built in `parse_cips_index_command()`
(`cli/src/main.rs:783-822`) — reuse its connection, do not open a second one
(requirement 55) and do not add a per-ref `SELECT` for uid existence
(requirement 56).

**The tooling reports; it never repairs** (PRD §4.6). No auto-correction, no
auto-rename, no normalization table, no "did you mean" substitution — not for
bad locations, not for headword spellings, not for unresolved xref targets. The
generated JSON reproduces the source strings verbatim, and warning lines quote
the offending value **exactly as it appears**, typos included, so the author can
find the row. (The existing lowercase in `parse_sutta_ref()`, `latinize()` in
sort keys and `make_normalized_id()` are presentational/internal and are not
affected by this rule — none of them changes stored text.)

**Two traps, both measured:**

- **Do not use `title_map` as the uid-existence oracle.** It inserts only rows
  whose `title` is `Some` (`cli/src/main.rs:~805`), so a pli/ms sutta with a
  NULL title would be misreported as `UnresolvedUid`. There are 0 such rows
  today, so the expected figures stand — but build a separate
  `HashSet<String>` of uids in the **same** loop over the same query result.
- **`content_json` keys are full segment ids** (`"dn33:1.11.0"`, uid prefix
  included). Compare against the whole `sutta_ref`, unsplit. Splitting on `:`
  and comparing the tail fails all 1,579 refs.

- [x] 2.0 CLI `parse-cips-index`: validate every paragraph location and report a summary
  - [x] 2.1 Define the `SuttaSegments` enum and an `AnchorValidation { checked, ok, unresolved_uid, no_segments, missing_segment, warnings: Vec<String> }` result struct in `parse_cips_index.rs`.
  - [x] 2.2 Add `fn validate_anchors(index: &[TopicIndexLetter], segments_lookup: &dyn Fn(&str) -> SuttaSegments) -> AnchorValidation` — walk every `type == "sutta"` ref, skip those whose `sutta_ref` has no `:`, and for the rest resolve the uid (the part before `:`) through the closure and classify into the three categories of requirement 52. Each warning line names the headword, the sub-topic and the failing `sutta_ref`.
  - [x] 2.3 Widen `parse_cips_index()` and `parse_cips_to_json()` with the optional `segments_lookup` parameter and thread it through. Keep `title_lookup` as it is.
  - [x] 2.4 In `parse_cips_to_json()`, after the existing warning/error printing, run the validation when the closure is `Some` and print the summary line followed by the individual warnings (requirement 53). When it is `None`, print one line saying anchor validation was skipped because no database was given (requirement 57).
  - [x] 2.5 Ensure the result is **advisory only**: it must not touch `ValidationResult::errors`, must not short-circuit the JSON write, and must not change the exit status (requirement 54).
  - [x] 2.6 In `cli/src/main.rs`, extend the `if let Some(db) = db_path` branch to also build the segment lookup: a `RefCell<HashMap<String, SuttaSegments>>` cache over the same `SqliteConnection`, loading `content_json` for `{uid}/pli/ms` **lazily, per referenced uid** (requirement 55). Answer `UnresolvedUid` without a query (requirement 56) from a **new `HashSet<String>` of uids collected in the same loop that fills `title_map`** — *not* from `title_map` itself, which drops rows with a NULL title.
  - [x] 2.7 Parse the loaded `content_json` into its segment keys — the top-level JSON object's keys, which are **full segment ids** (`"dn33:1.11.0"`), matched against the whole `sutta_ref` unsplit. Treat an empty or absent `content_json` as `NoSegments`. Note in a comment that the page's `id` attributes actually come from `content_json_tmpl` (a key without a template renders with no `id`), so this check is an approximation covered at runtime by requirement 23; measured, 0 of the 32 referenced suttas have an untemplated content key.
  - [x] 2.8 Pass `None` for the segment lookup in the `else` (no `--db-path`) branch, so the existing degraded mode keeps working.
  - [x] 2.9 Add unit tests driving `validate_anchors` with a synthetic in-test closure: one exact hit, one missing segment, one unresolved uid, one no-segments text, and one ref without a `:` (must not be counted as checked).
  - [x] 2.10 Confirm the report-don't-repair rule holds in the code as written (PRD §4.6, requirements 59–61): `validate_anchors` and the summary printer take `&` references and mutate nothing; every warning line interpolates the source value directly with no substitution; and the only field this feature adds to a ref is `suffix`, which leaves `sutta_ref` and `title` untouched. Add a unit test that a validation run over an index containing a bad location leaves the index bytes identical.
  - [x] 2.11 Run `cd cli && cargo test`.

---

### Specs for 3.0 — regeneration and the backend struct

**The backend struct must stay field-identical to the CLI's** (requirement 49).
Because the field is optional, newer code parses the older JSON, which is what
makes tasks 1–2 and task 4 independently landable (§6.10).

**Acceptance is a diff — but establish the baseline first.** The committed JSON
may not be byte-reproducible from the current CSV + current code; if it is not,
that pre-existing staleness will show up in the diff and be blamed on the sort
changes of 1.6–1.8. So regenerate **once on an unmodified tree** and confirm
`git diff` is empty *before* touching the parser:

```sh
git stash            # or work from a clean checkout
make parse-cips
git diff --stat assets/general-index.json   # must be empty — that is the baseline
git checkout assets/general-index.json; git stash pop
```

Then, with the changes applied, compare pretty-printed forms (the committed
JSON is minified — one line):

```sh
jq -S . assets/general-index.json > /tmp/before.json
make parse-cips
jq -S . assets/general-index.json > /tmp/after.json
diff /tmp/before.json /tmp/after.json   # only "suffix" additions expected
```

(`jq -S` sorts object keys, which makes the diff readable; array order — the
thing the sort changes could break — is preserved and still compared.)

Expected: **55 added `suffix` values across 25 groups**, nothing else
(success metric 9). Any reordering means task 1.6–1.8 changed a sort order and
must be reverted.

- [ ] 3.0 Regenerate `assets/general-index.json` and add the `suffix` field to the backend struct
  - [ ] 3.1 Add the identical `suffix: Option<String>` field (same doc comment, same `skip_serializing_if`) to `TopicIndexRef` in `backend/src/topic_index.rs:20`, and confirm `cd backend && cargo test` still passes against the **old** JSON — proving the field is genuinely optional.
  - [ ] 3.2 Establish the clean-tree baseline first (the `git stash` block above), then capture the pretty-printed "before" copy of `assets/general-index.json`, run `make parse-cips` and diff. Confirm the only changes are added `suffix` keys.
  - [ ] 3.3 Count the suffixed refs and their groups in the regenerated JSON and confirm **55 refs / 25 groups**. If the numbers differ, the scoping rule (requirement 12) was implemented differently — fix it in task 1.5 rather than accepting the new number.
  - [ ] 3.4 Confirm the named check from success metric 9 by inspection of the JSON: headword **feet**, sub-topic **Buddhas'**, five `dn30` refs carrying `a`–`e` in the order `dn30:1.4.0`, `1.7.0`, `1.10.0`, `1.16.0`, `1.19.0`.
  - [ ] 3.5 Confirm the anchor-validation summary printed by that same run reads `1579 checked, 1573 ok, 0 unresolved uid, 0 no segments, 6 missing segment` and names all six locations (success metric 12), and that the command exited 0 and wrote the file.
  - [ ] 3.6 Rebuild (`make build -B`) so `include_str!(CIPS_GENERAL_INDEX_JSON)` picks up the regenerated file, and run `make rust-test`.

---

### Specs for 4.0 — Topic Index rendering

**Target shape** (requirement 2):

```
abandoning (pajahati, pahāna)
  sense bases:
    SN 35.24 Pahānasutta

  striving:
    DN 33 Saṅgītisutta
```

**Rules that are easy to get wrong:**

- The colon is **presentation only** — append it *outside* the string passed to
  `highlight_query_terms()`, so it never enters search matching (requirement 1).
  `highlight_query_terms()` returns rich text; concatenating `":"` after the call
  is correct and keeps the colon unhighlighted.
- The gap goes on `Layout.topMargin` of the sub-topic `ColumnLayout`, **not** on
  the `ListView.spacing`, which would also space the headwords (§6.9).
- No gap for `sub_topic.index === 0` (requirement 3) and none for entries with an
  empty or `"—"` sub (requirement 4).
- Scale the gap with `root.pointSize`, not a hardcoded pixel value (§7).
- The headword-highlight rectangle's height is driven by `headword_column.height`
  — confirm it still covers the whole entry after the margins change
  (requirement 5).

**Dependencies:** the `suffix` field from 3.1/3.2 (the QML reads
`modelData.suffix`; with an older JSON it is simply `undefined` and no suffix is
appended).

- [ ] 4.0 Topic Index window: sub-topic colon, vertical rhythm, and segment-id-free reference labels
  - [ ] 4.1 In the sub-topic `Text` (`TopicIndexWindow.qml:461-473`), append `":"` after the `highlight_query_terms()` result when the sub is non-empty and not `"—"`. Leave the `visible:` binding as it is.
  - [ ] 4.2 Add `Layout.topMargin` to the sub-topic `ColumnLayout` (`:451-459`): roughly one blank line (`root.pointSize * 1.2` or similar, tuned by eye) when `sub_topic.index > 0` **and** the entry has a real sub label; `0` otherwise.
  - [ ] 4.3 Increase the links `Flow`'s `Layout.leftMargin` (`:477`) modestly for entries that have a sub label, keeping `0` for those that do not, so the parent/child relation reads clearly (requirement 5).
  - [ ] 4.4 Confirm the headword highlight `Rectangle` still spans the full entry after the margin change — it is bound to `headword_column.height`, so verify the margins are inside that column and not on it.
  - [ ] 4.5 Change `format_sutta_ref()` (`:180-188`) to strip the segment id: take the part before `:` before applying the existing letter/number spacing (requirements 6, 7). Keep the function's name and its `toUpperCase()` fallback.
  - [ ] 4.6 In the ref delegate's `text` binding (`:487-494`), append `" (" + modelData.suffix + ")"` when `modelData.suffix` is a non-empty string (requirements 9, 10). Guard for `undefined` so an older JSON renders unchanged. Do **not** compute collisions in QML.
  - [ ] 4.7 Confirm the `xref` branch of the same binding, its styling and its click handler are untouched (requirements 4, 13).
  - [ ] 4.8 Run `make qml-test` and `qmllint` on `TopicIndexWindow.qml`; confirm no new warnings.

---

### Specs for 5.0 — routing the segment id through `anchor`

**The one-line bug** (§6.1): `TopicIndexWindow.qml:203` writes

```qml
segment_id: sutta_ref.includes(":") ? sutta_ref : ""
```

into the result data. Nothing reads `segment_id`. It must become
`anchor: <full segment id, e.g. "dn33:1.11.0">` and the old key must be **removed**
(requirement 15) so there is one mechanism, not two.

**Both open modes** must be exercised (requirement 16): the new-window path
`open_sutta_search_window_with_result()` (`sutta_bridge.rs:3660` →
`cpp/gui.cpp:210` → `signal_open_sutta_search_window` →
`WindowManager::open_sutta_search_window_with_query()`,
`cpp/window_manager.cpp:497`) passes the JSON string verbatim **and lands on the
same `show_result_in_html_view_with_json`**, so no new plumbing is needed there.
It still needs its own check, for a sharper reason than "a different entry
point": the fresh window takes the tab-0 **update** branch
(`SuttaSearchWindow.qml:1210-1240`), whose own comment records that the webview
is not found the first time while the window objects are still being
constructed. If `get_item()` returns nothing there, `data_json` — and the anchor
with it — is dropped silently and the sutta opens at the top. This is the one
place in the anchor path with a real risk of silent loss (§6.19).

**The already-open case is narrower than it looks.** The anchor is part of the
URL (`_Desktop.qml:137-142`), and `onData_jsonChanged` (`:241`) calls
`load_sutta_uid()` on every change — so a *different* anchor on the same open
sutta already changes the URL, reloads, and fires the existing `scroll_timer`.
**Requirement 17 needs no new code for that case.** The only inert case is
identical `data_json`: same uid **and** same anchor, where no change signal
fires at all.

This distinction is load-bearing. A direct `scroll_to_anchor()` keyed on the
**uid alone** runs *before* the pending reload, against the **previous** page's
DOM — and once task 6 lands, it can insert a notice that the incoming page then
discards, or scroll the outgoing page. Compare uid and anchor together.

**Ordering:** the anchor is applied **after** `convert_verse_ref_to_uid()`
(requirement 38) — the existing code already splits the uid off before the
conversion, so keep that shape and carry the full `sutta_ref` separately.

**Dependencies:** none beyond task 4 sharing the same file.

- [ ] 5.0 Route the segment id through the existing `anchor` path and re-scroll an already-open sutta
  - [ ] 5.1 In `TopicIndexWindow.qml`'s `open_sutta()` (`:189-212`), replace the `segment_id` key with `anchor`, carrying the **full** segment id (`dn33:1.11.0`), and empty string when the ref has no `:` (requirement 37).
  - [ ] 5.2 Verify by reading `SuttaSearchWindow.qml:367` that `new_tab_data()` copies `anchor` from the result data onto the tab, and `:1228` that the existing-tab update path does too — i.e. that no further plumbing is needed for the in-place mode.
  - [ ] 5.3 Confirm the new-window mode: the trace to `WindowManager::open_sutta_search_window_with_query()` (`cpp/window_manager.cpp:497`) shows it reaches the same `show_result_in_html_view_with_json`, so no pass-through is missing. Instead check the risk named above — that the tab-0 update branch (`SuttaSearchWindow.qml:1210-1240`) finds its webview on a freshly constructed window; if `get_item()` returns nothing, `data_json` (and the anchor) is dropped without a log line. Add a `logger.warn()` there if it can miss.
  - [ ] 5.4 In `show_result_in_html_view()` (`SuttaSearchWindow.qml:1095`), extend the existing `already_open_uid` capture — which today exists only for the find-bar (`:1096-1108`) — to capture the current item's **`anchor` as well**, and invoke that item's `scroll_to_anchor()` directly **only when the uid and the anchor are both unchanged** (§6.6, §6.15). Do **not** key this on the uid alone: a same-uid/different-anchor click already reloads via the URL, and an eager call would run against the outgoing DOM.
  - [ ] 5.5 That direct call is what makes clicking the *same* link twice, after scrolling away, re-scroll rather than do nothing (§6.15 / open question 4 — the PRD's own recommendation, one line). Verify it runs *after* the `data_json` write, so the no-change case is unambiguous.
  - [ ] 5.6 Confirm a ref with no segment id still opens at the top with no `anchor` query parameter and no highlight (requirement 37) — the empty-string anchor must fall through the `root.anchor && root.anchor.length > 0` guards in both wrappers.
  - [ ] 5.7 Check every log call added or touched in this group uses `logger.<level>()` with a **single concatenated string**.
  - [ ] 5.8 Run `make qml-test` and `qmllint` on both changed QML files.

---

### Specs for 6.0 — in-page resolution, highlight and notice

**Put the logic in TypeScript, not in a QML string literal.** The two wrappers
today carry byte-identical copies of `scroll_to_anchor()`'s JS
(`_Desktop.qml:196-226`, `_Mobile.qml:306-336`); adding the walk, the highlight
and the notice to both would triple that duplication and put non-trivial logic
beyond the reach of Jest. Instead add `src-ts/anchor_jump.ts`, export

```ts
// Returns "exact" | "fallback:<used-id>" | "missed"
export function jump_to_segment(requested: string): string
```

expose it as `window.ssp_jump_to_segment`, and reduce each wrapper's
`scroll_to_anchor()` to a `runJavaScript("window.ssp_jump_to_segment(<id>)", cb)`
with the existing three-branch body kept only as a fallback for pages that do not
load the bundle (book chapters, dictionary pages).

**The candidate walk** (requirement 19), operating on ids **as they exist in the
loaded page** (requirement 23):

1. exact: `document.getElementById(requested)`;
2. decrement the **last** numeric component down to `0`
   (`1.7.9.10` → `1.7.9.9` … `1.7.9.0`);
3. the parent, exactly once (`1.7.9`);
4. stop. Never `1.7.8`, never `1.7`.

**Step 3 never fires against the current data, and that is expected** —
verified: neither `dn33:1.7.9` nor `dn20:4` exists as a segment key, because
Bilara emits headings as `x.y.z.0`, never as the bare parent. Both acceptance
cases resolve at step 2. Implement step 3 anyway (it is two lines), but do not
debug its silence, and do not treat the §8.5 checks as covering it. Cover it in
`anchor_jump.test.ts` instead, where a synthetic DOM can exercise it (6.13).

A last component that is not numeric skips step 2 (requirement 24). Use
`getElementById` throughout — a colon is a valid id but **not** a valid CSS
selector fragment, which is why the existing third branch throws (§6.3); wrap
that legacy branch in `try/catch` while you are there.

**Notice rules** (requirements 20, 21, 25–36) — one component, two forms:

| | fallback form | give-up form |
|---|---|---|
| text | `Referenced location dn20:4.11 not found. This location dn20:4.10 is the closest fallback.` | `Referenced location dn20:4.11 not found.` |
| placement | sibling **before** `el.closest('p, li, h1, h2, h3, blockquote') \|\| el` | first child of `#ssp_content` |

Both: real selectable text nodes (never CSS `content:`), full ids
(`dn20:4.11`, not `4.11`), body-size and theme-aware, **no auto-fade**, a
dismiss "×" with an `aria-label` and a ≥ 44 px mobile hit area, **at most one in
the page** (remove any existing notice before inserting), inserted **before** the
scroll, and never shown on an exact hit or on a reference with no location.

Attach the dismiss handler to the button or delegate from the notice root —
never to a captured text node, because the find bar splices
`.ssp-find-highlight` spans into the notice's text (§6.13).

**Never inject a block element into `<span class="segment">`** — in the
multi-column layouts it is the grid container holding the per-column `colcell`
spans, and a block child adds a phantom grid item that breaks the row's
alignment (§6.14, requirement 33). This must be checked in **Columns** layout,
not only Lines.

**Logging** (requirement 22) belongs in QML: have the wrapper pass a callback to
`runJavaScript` and log `logger.info()` on `fallback:` and `logger.warn()` on
`missed`, naming the uid and both ids in full form.

**Dependencies:** task 5 (an anchor must actually arrive).

- [ ] 6.0 In-page anchor resolution: fallback walk, paragraph highlight, and the notice component
  - [ ] 6.1 Create `src-ts/anchor_jump.ts` with `jump_to_segment(requested)` implementing the walk above, plus `candidate_ids(requested): string[]` as a separately exported pure function so the walk is unit-testable without a DOM.
  - [ ] 6.2 Handle the non-numeric-last-component case (requirement 24) and a requested id with no `.` at all — neither may crash or loop.
  - [ ] 6.3 Implement the highlight: add a class (e.g. `ssp-anchor-highlight`) to the resolved element, scoped so it changes background only and alters no geometry (requirement 18).
  - [ ] 6.4 Implement `show_anchor_notice(requested, used_or_null)`: build the element with real text nodes, remove any existing `.ssp-anchor-notice` first (requirement 32), place it per the table above (requirement 33), wire the "×" with an `aria-label` and a delegated click handler (requirements 31, §6.13), and insert it **before** the scroll (requirement 34).
  - [ ] 6.5 Make `jump_to_segment` return `"exact"` / `"fallback:<id>"` / `"missed"` and show no notice on `"exact"` (requirement 35). On `"missed"` scroll the page to the top and place the give-up notice as the first child of `#ssp_content` (requirement 20).
  - [ ] 6.6 Register `window.ssp_jump_to_segment = jump_to_segment` from `src-ts/simsapa.ts`'s init, next to the other page-level registrations.
  - [ ] 6.7 Create `assets/sass/_anchor_jump.scss` with the highlight and notice styling: theme-aware light/dark pair modelled on `_find.scss:242-258`, a highlight colour **visually distinct** from the yellow/green find-match colour (§7), the notice as a full-width block in normal flow with its own subdued background and a clear boundary — **never** `position: fixed` (§7), body-size type (requirement 29), and no `user-select: none` on the message (requirement 26).
  - [ ] 6.8 Add `@include meta.load-css("anchor_jump")` to `assets/sass/suttas.sass` beside the existing `find` include (`:119`) and run `make sass`.
  - [ ] 6.9 Reduce `scroll_to_anchor()` in `SuttaHtmlView_Desktop.qml:196` to call `window.ssp_jump_to_segment` via `runJavaScript` with a result callback, keeping the existing three-branch body as the no-bundle fallback and wrapping its `document.querySelector('${root.anchor}')` branch in `try/catch` (§6.3).
  - [ ] 6.10 Apply the identical change to `SuttaHtmlView_Mobile.qml:306`. The result-callback form is **already used unconditionally on both platforms** (`SuttaSearchWindow.qml:147`, `get_current_scroll_position()`), so no platform branch is expected; if it nevertheless proves unreliable through the native `QtWebView`, log from JS in a way that reaches the app log rather than dropping the requirement-22 logging silently.
  - [ ] 6.11 In both wrappers' callbacks, emit `logger.info()` for a resolved fallback (naming requested and used ids) and `logger.warn()` for a miss (naming the uid and the anchor) — single concatenated string arguments (requirement 22).
  - [ ] 6.12 Confirm the URL-fragment half of the anchor URL (`#dn33:1.11.0`, `_Desktop.qml:141`, `_Mobile.qml:251`) is not double-encoded such that native scrolling silently fails (§6.4). Adjust only if it is.
  - [ ] 6.13 Add `src-ts/anchor_jump.test.ts`: `candidate_ids` for `1.7.9.10` (stops after `1.7.9`, never `1.7.8`), for a non-numeric tail, and for a bare id; a jsdom case where **only** the parent id exists, since real data never exercises that branch; jsdom tests for the notice — one instance only after two calls, dismissal works after text nodes have been spliced, give-up placement is the first child of `#ssp_content`, and the notice is inserted before the scroll call.
  - [ ] 6.14 Run `npx webpack` and `make js-test`.
  - [ ] 6.15 Re-read the generated markup for **Columns** layout (`helpers.rs:2426-2438`) and confirm the chosen insertion point can never land inside `<span class="segment">` (§6.14, success metric 8). Note this as a user-facing visual check for task 8.

---

### Specs for 7.0 — the "Show references" option

**Precedence, highest first** (requirement 43):

1. an explicit `show_references` request parameter;
2. an `anchor` parameter on the request (forces on for that render);
3. the persisted `SuttaDisplayDefaults` value (new, default `false`).

**The signature must change.** `resolve_sutta_display_options(&sutta,
show_references: bool, &overrides)` (`app_data.rs:385`) cannot express "unset".
Move it into `SuttaDisplayOverrides` as `show_references: Option<bool>` — the
shape consistent with `layout` / `columns` / `repeat_pali` — and drop the
positional `bool` from both `resolve_sutta_display_options` and
`SuttaDisplayOptions::resolve`. Three call sites are affected: `api.rs:741`,
`api.rs:1667`, `app_data.rs:404`.

Then `api.rs:731`'s `let show_references = anchor.is_some();` becomes rule 2:
set `overrides.show_references = Some(true)` when an anchor is present and no
explicit parameter was given — **narrowed, not deleted**.

**Two things are already free, so do not build them.** `SuttaDisplayDefaults`
carries a **struct-level** `#[serde(default)]` (`app_settings.rs:468`), so old
settings rows load with no per-field attribute — adding one implies the struct
attribute is missing and should not be added. And `POST
/save_sutta_display_settings` (`api.rs:1717`) deserializes the body straight
into that struct while the client already posts its whole settings object, so
**C17 requires no route, handler or payload change at all** — only the client
field of 7.9.

**Client contract.** `SuttaDisplaySettings` (`display_settings.ts:24`) gains
`show_references: boolean`, `built_in_defaults()` sets it `false`, and
`merged_settings()` reads it from `defaults_json` — with a
`typeof === "boolean"` check, **not** the `defaults_json.x || base.x` shape the
neighbouring string fields use (`:106`), which silently discards an explicit
`false`. It is harmless while the base is `false`, but the two boolean reads
(here and in 7.10) must not diverge. `init_display_settings()`
must seed it from the **effective** `SUTTA_DISPLAY.show_references` (true on an
anchor page) exactly as it already does for `layout` and `repeat_pali`
(`:1005-1015`) — and that seeding must **not** trigger a POST (requirement 46).

The rerender handler widens from `(layout, repeat_pali)` to
`(layout, repeat_pali, show_references)` (`display_settings.ts:69/73/79`,
`simsapa.ts:170-172`), and `refetch_with_params()` (`content_reload.ts:218`)
takes the value as a parameter instead of reading it back from
`SUTTA_DISPLAY` — otherwise a toggle would re-send the old value and requirement
45 would fail.

**`show_references` is not read by the renderer beyond
`options.show_references`** (`app_data.rs:587`), so nothing in `helpers.rs`
changes (§6.2).

**Dependencies:** independent of tasks 1–6, but requirement 44 (the scroll must
work with references **off**) is only testable once task 6 exists.

- [ ] 7.0 "Show references" as a persisted display option with anchor-forced precedence
  - [ ] 7.1 Add `show_references: bool` to `SuttaDisplayDefaults` (`app_settings.rs:469`) and set it `false` in the `Default` impl (`:485`). **No per-field `#[serde(default)]`** — the struct already carries one at `:468`, which is what lets existing settings rows load (§6.17).
  - [ ] 7.2 Add `show_references: Option<bool>` to `SuttaDisplayOverrides` (`sutta_display.rs:16`) and remove the positional `show_references: bool` parameter from `SuttaDisplayOptions::resolve` (`:41`), resolving it as `overrides.show_references.unwrap_or(app_settings.sutta_display.show_references)`.
  - [ ] 7.3 Remove the positional parameter from `AppData::resolve_sutta_display_options` (`app_data.rs:385`) and update its internal call (`:404`).
  - [ ] 7.4 Update `sutta_html_response()` (`api.rs:719-741`): delete `let show_references = anchor.is_some();` and instead set `overrides.show_references = Some(true)` when `anchor.is_some()` — precedence rule 2. Update the call at `:741` and the helper it calls (`try_render_sutta_html_by_uid_with_overrides`).
  - [ ] 7.5 Update `/sutta_content_block` (`api.rs:1651/1667`) to put its existing `show_references: Option<bool>` query parameter into the overrides (precedence rule 1) rather than passing `unwrap_or(false)`.
  - [ ] 7.6 Check the shared GET-parameter parser in `sutta_display.rs` (the `layout` / `columns` / `repeat_pali` one) and decide whether `show_references` belongs there too; if it does, add it and use it from all three routes for consistency.
  - [ ] 7.7 Confirm `sutta_display_js()` (`app_data.rs:646-655`) now serializes the new field inside `defaults` for free, and that `SUTTA_DISPLAY.show_references` still carries the **effective** value.
  - [ ] 7.8 Add the control to `assets/templates/display_settings.html`: a `ds-label` "Show references" plus a `ds-segmented` with `data-setting="show-references"` and `Off` / `On` buttons, placed **after** the Repeat Pāli control and **before** Width (§7, requirement 39).
  - [ ] 7.9 In `src-ts/display_settings.ts`: add `show_references: boolean` to `SuttaDisplaySettings`, `false` in `built_in_defaults()`, the read in `merged_settings()` (`typeof === "boolean"`, **not** `||` — see above), a `set_show_references(v)` mirroring `set_repeat_pali()` (`:320-328`), the `case "show-references":` in the segmented-control switch (`:899`), and the `sync_controls()` line beside the repeat-pali one (`:438-441`). Note the segmented control's `data-value` arrives as a **string** (`"on"`/`"off"`) — convert once in the `case`, do not store the string.
  - [ ] 7.10 Seed it in `init_display_settings()` (`:1005-1015`) from `sd.show_references` — using `typeof sd.show_references === "boolean"`, not truthiness, so an explicit `false` is honoured — and confirm no POST is scheduled by that seeding (requirement 46).
  - [ ] 7.11 Widen `rerender_handler` to `(layout, repeat_pali, show_references)` (`:69`, `:73`, `:79`), update `request_rerender()`, and update the wiring in `src-ts/simsapa.ts:170-172`.
  - [ ] 7.12 Widen `refetch_with_params()` (`content_reload.ts:218-224`) to take `show_references` as a parameter instead of reading `SUTTA_DISPLAY.show_references` (requirement 45), leaving `build_content_block_url()` and the post-swap write-back (`:130`) as they are.
  - [ ] 7.13 Confirm `reset_all()` (`display_settings.ts:359-372`) requests a re-render when `show_references` changed, alongside its existing layout/repeat-pali comparison.
  - [ ] 7.14 Extend `src-ts/display_settings.test.ts` (the new setting's state, scope semantics, and that seeding does not POST) and `src-ts/content_reload.test.ts` (`:26`, `:31-33`, `:141-149` already assert on `show_references` in the URL — extend for the new parameter shape).
  - [ ] 7.15 Settle requirement 45's scope limit (PRD open question 5): the user's "off" choice lives in the page, so any **full reload** of that tab rebuilds the URL from the wrapper's still-set `root.anchor` and rule 2 forces references back on. Either clear `root.anchor` in `SuttaHtmlView_{Desktop,Mobile}.qml` once the jump has resolved, or record the behaviour as accepted in the doc (8.1). Do not leave it undecided.
  - [ ] 7.16 Confirm no work is needed for the persist path: `POST /save_sutta_display_settings` (`api.rs:1717`) takes the whole `SuttaDisplayDefaults` and the client posts its whole settings object, so C17 lands with 7.1 + 7.9. Verify by round-tripping in a Rust test rather than by inspection.
  - [ ] 7.17 Run `npx webpack`, `make js-test`, `make rust-test`, and `make build -B`.

---

### Specs for 8.0 — docs, tests, handover

`docs/sutta-display-settings-and-multi-column-view.md` is the doc of record for
the cogwheel panel; it must gain the new option, its scope semantics and the
requirement-43 precedence (§6.17). `PROJECT_MAP.md` must list the new
TypeScript and Sass files.

The PRD's success metrics 1–12 are almost all **visual, GUI checks** — per the
project rules the agent must not run the GUI, so this task ends by handing the
user a concrete checklist rather than by claiming them.

- [ ] 8.0 Tests, documentation and final verification
  - [ ] 8.1 Update `docs/sutta-display-settings-and-multi-column-view.md`: the **Show references** option, its Off/On control and placement, the three-level precedence of requirement 43, the `SuttaDisplayOverrides.show_references` signature change, and a note that the two full-page sutta routes still have no `show_references` parameter (only the anchor rule).
  - [ ] 8.2 Add a short section to the same doc (or a new `docs/` note, cross-linked) covering the anchor jump: the candidate walk and its deliberate stopping rule, the two notice forms, and the "never inject a block into `span.segment`" constraint.
  - [ ] 8.3 Update `PROJECT_MAP.md` with `src-ts/anchor_jump.ts`, `src-ts/anchor_jump.test.ts` and `assets/sass/_anchor_jump.scss`.
  - [ ] 8.4 Run the full suite: `make test` (rust + qml + js) plus `qmllint` on the three changed QML files; confirm no new warnings (success metric 13).
  - [ ] 8.5 Run `make build -B` and confirm a clean build.
  - [ ] 8.6 Re-read the PRD's §8 success metrics against the implementation and write the user a verification checklist naming the exact clicks for the ones only a human can confirm: metrics 1–11 (the two named fallback checks — *conditions (saṅkāra) → all beings sustained by* landing on `dn33:1.7.9.0`, and *Māra → attacks gathering of arahants* landing on `dn20:4.10`; the notice's selectability; Columns-layout alignment; the `feet` / `Buddhas'` `(a)`–`(e)` group; references forced on from the Topic Index and turned off in the cogwheel; and the restart-survival check). **Search for `conditions (saṅkāra)` — without the *h*.** The correct Pāli is *saṅkhāra*; the CSV misspells this one headword on 10 rows (the rest of the file spells it correctly), so the entry is findable only by the typo until the source is fixed — §C of [the corrections doc](./2026-08-11-144238-cips-paragraph-location-corrections.md). If the author corrects it first, this check and the sample warning line above both move to `saṅkhāra`. For metric 6 (the give-up notice) the checklist must say how to reach it, since no Topic Index link produces it directly: open a segment-carrying link, then switch the reading panel to a non-segmented text of the same sutta (`dn33/pli/cst`, `dn33/en/thanissaro`, `dn33/en/tw-caf_rhysdavids` all have an empty `content_json`) and jump again.
  - [ ] 8.7 Report which requirements are covered by automated tests and which rest on the manual checklist, so nothing is reported as verified that was not.

---

## Review Notes (internal consistency and risks)

All figures the PRD quotes (19,970 sutta refs; 1,579 segment-carrying; 25
collision groups / 55 members / max 5; 32 distinct uids; 1,573 ok / 6 missing /
0 unresolved / 0 no-segments, and the six named locations) were independently
re-derived during review and reproduce **exactly**, including the collision key
`(reference-before-the-colon, title)`. The line references throughout both
documents were spot-checked and are accurate.

- **Task 1.8 is the only cleanup that can silently change the shipped JSON —
  and its equivalence has now been checked.** `compare_locators` (`:205-231`)
  is exactly `(book_order_index, Vec<u32>)` compared lexicographically; the
  "shorter wins" tie-break *is* `Vec<u32>`'s derived `Ord`, and both sorts are
  stable. The tuple key is safe. Task 3.2's diff remains the guard — run against
  a **clean-tree baseline** first, or a pre-existing staleness in the committed
  JSON gets misattributed to this change.
- **Task 5.4 was corrected during review.** The original framing ("when the uid
  is unchanged, no `LoadSucceededStatus` fires") is wrong: the anchor is part of
  the URL, so a different anchor on an open sutta already reloads and scrolls.
  Keying the direct `scroll_to_anchor()` on the uid alone would fire it against
  the *outgoing* DOM — with task 6 in place, that can plant a notice the
  incoming page discards. The guard is uid **and** anchor.
- **Requirement 19's step 2 is dead against real data** (no `dn33:1.7.9`, no
  `dn20:4` — Bilara headings are `x.y.z.0`). Implement it, test it in jsdom, do
  not expect the device checks to exercise it.
- **Two CLI traps, both measured**: `title_map` is not an existence oracle (it
  drops NULL-title rows; 0 today), and `content_json` keys are full ids
  (`dn33:1.11.0`) — see the 2.0 spec.
- **The tooling reports, the author decides** (PRD §4.6). The CIPS source data
  is never repaired, renamed or normalized by the pipeline — defects are
  reported verbatim and sent to the index author to evaluate. This is why §C of
  the corrections document exists rather than a fix, and why 2.10 asserts the
  validator mutates nothing. It applies to any defect found later, not only the
  ones catalogued today.
- **The collision key in Rust (1.2) and the displayed label in QML (4.5/4.6) must
  agree.** They are two implementations of one rule in two languages; if they
  drift, suffixes appear on labels that do not look alike (or are missing where
  they do). Both sites carry a comment naming the other.
- **The `anchor` rename (5.1) is the whole of the reported bug** and is
  independently shippable — it is worth confirming the exact-hit path works
  before task 6 adds the fallback machinery on top.
- **Requirement 44 is a trap by construction**: with rule 2 in place, every Topic
  Index arrival has references on, so "the scroll works with references off" is
  untestable by simply opening a link. The testable path is the one the PRD
  names — open from the index, switch references off in the cogwheel, then jump
  to another location in the same sutta (task 8.6).
- **Two `id` attributes exist in the page** (`id="dn33:1.11.0"` on the segment
  wrapper, `id="1.11.0"` on the reference anchor inside it, and the latter only
  when references are on). Everything in task 6 targets the **full colon-bearing
  id** — the short one also collides across columns (§6.12).
- **Open question 2 affects task 3.5's expected numbers.** If the CSV's six bad
  locations are corrected before this ships, the summary line changes and the
  acceptance figures in 3.3/3.5 must be re-baselined against the corrected data.
  Nothing else in the plan depends on the answer.
- **Open question 3** (mobile `runJavaScript` reliability for a colon-bearing id)
  is narrower than stated: the *callback* form is already used unconditionally
  on both platforms (`SuttaSearchWindow.qml:147`), so requirement 22's logging
  needs no mobile channel. What remains open is only whether the scroll itself
  lands on the native webview — answered inside task 6.10.
- **Open question 5** (new): requirement 45's choice does not survive a full
  tab reload, because `root.anchor` stays set and re-forces the references on.
  Task 7.15 forces a decision either way.
- **C17 has no work of its own.** The save route takes the whole
  `SuttaDisplayDefaults` and the client posts its whole settings object, so the
  persist path lands with 7.1 + 7.9. The component table's `C14, C16 → C17`
  dependency stands, but the box is essentially empty (task 7.16 just verifies
  it).
