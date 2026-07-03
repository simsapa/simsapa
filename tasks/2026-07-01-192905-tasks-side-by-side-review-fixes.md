# Tasks: Side-by-Side View — Review Fixes

Based on the implementation review (2026-07-03) of PRD
`2026-07-01-192905-prd---side-by-side-translation-view.md` and task list
`2026-07-01-192905-tasks-side-by-side-translation-view.md`.

## Review Findings Being Addressed

1. **Potential reader-writer deadlock on `app_settings_cache`** — nested read
   guards in the render path (`render_sutta_html_by_uid_with_overrides` :716 →
   `render_sutta_content` :597 → `sutta_to_segments_json` :285 /
   `sutta_display_js` :565) can deadlock against the new high-frequency
   `save_sutta_display_defaults` write lock (:208) on writer-preferring RwLock
   implementations (macOS pthreads); Linux/glibc masks it in development.
2. **Client column state diverges from server-resolved columns** — the
   Lines-mode non-segmented drop (`resolve_sutta_display_options`, :341) is
   never communicated back on `/sutta_content_block` fetches, so per-column-
   index CSS vars land on wrong columns, the bar shows a phantom column, and
   the PRD FR 8 "exclude with a notice" has no notice in this path.
3. **"+" button ignores the Lines-mode rule** (subset of 2) — `next_unshown`
   (`column_bar.ts:109`) can suggest a non-segmented text in Lines mode.
4. **No debounce on slider autosave** — every slider `input` tick /
   color-picker drag POSTs `/save_sutta_display_settings` (write lock + full
   `app_settings` row rewrite); dozens per second, and the trigger for 1.
5. **PRD documentation drift** — Solo layout and Repeat Pāli were adopted
   during implementation but the PRD still lists Repeat Pāli under §5
   Non-Goals and describes a two-mode `SuttaLayout`.
6. **Error-behavior parity** — unknown column uid is a 404 with a message on
   `/sutta_content_block` but a generic 200 "Rendering error" page on the
   full-page routes (`app_data.rs:735`).
7. **Task-list housekeeping** — parent `6.0` checkbox left unticked.

## Relevant Files

- `backend/src/app_data.rs` — the nested `app_settings_cache` read-guard sites
  (`render_sutta_html_by_uid_with_overrides` :716, `render_sutta_content`
  :597, `sutta_to_segments_json` :285, `sutta_display_js` :565), the
  `save_sutta_display_defaults` write path (:208), the Lines-mode column drop
  in `resolve_sutta_display_options` (:341), and the full-page error mapping
  (:735).
- `backend/src/sutta_display.rs` — `SuttaDisplayOptions`; the resolved column
  list that must be reported back to the client.
- `bridges/src/api.rs` — `/sutta_content_block` route (response gains the
  resolved-columns channel); full-page routes' error mapping.
- `src-ts/content_reload.ts` (+ `content_reload.test.ts`) — `fetch_content_block`
  adopts the server's resolved columns; drop notice.
- `src-ts/column_bar.ts` (+ `column_bar.test.ts`) — `next_unshown` must respect
  `option_disabled_reason`; bar re-render from adopted server state.
- `src-ts/display_settings.ts` (+ `display_settings.test.ts`) — debounce in
  `post_settings` / `on_setting_changed`; flush-on-close semantics.
- `backend/tests/test_render_sutta_content.rs` — coverage for the extracted
  no-nested-lock render path (explicit options, unchanged output).
- `tasks/2026-07-01-192905-prd---side-by-side-translation-view.md` — §5/§9
  v2.1 amendment note.
- `tasks/2026-07-01-192905-tasks-side-by-side-translation-view.md` — tick 6.0.
- `docs/sutta-display-settings-and-multi-column-view.md` — document the
  resolved-columns response contract, debounce, and locking rule.
- `docs/simsapa-localhost-api-search-endpoints.md` — `/sutta_content_block`
  response addition.

### Notes

- Staging rule: after each top-level task the app must compile
  (`make build -B`) and the relevant tests pass; run tests only after all
  sub-tasks of a top-level task are done.
- Rust tests: `cd backend && cargo test` (real appdata DB at the SIMSAPA_DIR
  path in `AGENTS.md`). TS: `npx jest`, then `npx webpack`. When the app is
  running, curl the live localhost API (port from `api-port.txt`).
- Findings 2 and 3 share one root cause — the client never learns the
  server's resolved column list — so task 3.0 closes both with one contract
  change.
- The deadlock (finding 1) does not reproduce on Linux/glibc
  (reader-preferring); the fix is verified structurally (no nested
  read-guard across a re-locking call) plus unchanged render output, not by
  reproducing the hang.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this
markdown file by changing `- [ ]` to `- [x]`. This helps track progress and
ensures you don't skip any steps.

Update the file after completing each sub-task, not just after completing an
entire parent task.

## Tasks

### 1.0 Eliminate nested `app_settings_cache` read guards in the render path

**Specs.** `app_settings_cache` is a `std::sync::RwLock`; re-acquiring a read
guard while one is already held on the same thread may deadlock when a writer
is queued (writer-preferring implementations — macOS pthreads — guarantee it;
Linux/glibc masks it). The render path currently nests:
`render_sutta_html_by_uid_with_overrides` (read at `app_data.rs:716`, held for
the whole body) → `resolve_sutta_display_options` (scoped read :321 — already
fine) and `render_sutta_content` (read at :597, held for the whole body) →
`resolve_column_suttas`/`render_content_block_for_columns` →
`sutta_to_segments_json` (read :285) and `sutta_display_js` (read :565), plus
`get_theme_name()` (own read). The concurrent writer is
`save_sutta_display_defaults` (write :208) POSTed from the page on every
settings change. **Rule to establish: a settings read guard is scoped to
copying the needed values out; never held across a call into a function that
may lock the cache.** Render output must be byte-identical — the existing
exact-match tests in `test_render_sutta_content.rs` are the regression net.

**Depends on:** nothing (first stage).

- [ ] 1.0 Eliminate nested `app_settings_cache` read guards in the sutta render path (finding 1)
  - [x] 1.1 In `render_sutta_content` (`app_data.rs:597`): replace the whole-body guard with a scoped block that copies out the needed values (`sutta_font_size`, `sutta_max_width`, `show_bookmarks`) and drops the guard **before** `resolve_column_suttas` / `render_content_block_for_columns` / `sutta_display_js` / `get_theme_name()` run. Note the values are read once up front even though used later — acceptable staleness within one render.
  - [x] 1.2 In `render_sutta_html_by_uid_with_overrides` (`app_data.rs:716`): the guard is only needed for `theme_name_as_string()` — copy `body_class` out in a scoped block and drop the guard before `resolve_sutta_display_options` and `render_sutta_content` are called.
  - [x] 1.3 Audit the rest of the sutta render/content-block call graph for the same pattern (grep `app_settings_cache.read` in `app_data.rs` and check each site reachable from `render_sutta_content_block`, `render_sutta_content`, `sutta_html_response`, and the QML bridge render path): confirm every remaining read guard is scoped to value extraction and none is held across a re-locking call. Fix any found; leave non-render sites alone. *(Found and fixed a second real nesting on the book path: `render_book_spine_html_by_uid` held its guard across `render_book_spine_item_html`, which held its own guard across `get_theme_name()` — a double nesting. Also scoped the guards in `render_word_html_by_uid` and `sutta_to_segments_json` for rule consistency (no re-locking callee today, but one refactor away). The `get_cached_*` getters use expression-temporary guards — fine; bridge/QML sites are scoped getters outside the render graph — left alone.)*
  - [x] 1.4 Add a short comment at the `app_settings_cache` field declaration (`app_data.rs:88`) stating the scoping rule (guards are for copying values out; never hold one across a call that may re-lock — std RwLock read-read re-entry can deadlock against a queued writer).
  - [x] 1.5 `make build -B`; `cd backend && cargo test` — the exact-match render tests must pass unchanged (byte-identical output proves the refactor is behavior-neutral). Verification is structural (no guard held across a re-locking call) — the hang does not reproduce on Linux/glibc. *(Build clean; all backend test suites pass, 0 failures — the exact-match render tests confirm byte-identical output.)*

### 2.0 Debounce the settings autosave POSTs

**Specs.** `display_settings.ts`: `on_setting_changed()` (:247) runs on every
slider `input` tick and color-picker drag tick, and calls `post_settings()`
(:226) directly when scope is `save_default` — dozens of POSTs per second,
each taking the settings write lock and rewriting the full `app_settings` row
(the trigger side of finding 1). Fix: a single trailing debounce timer
(~300 ms) inside the POST path, so `apply_css_vars()` stays instant while the
persistence coalesces. **Immediate flush** (cancel the timer, POST now) on:
scope switch local→default (`on_scope_changed`, :259 — FR 19's "must
immediately POST" contract), Reset all, and `pagehide` (navigation away must
not lose the last debounced change; note prev/next navigation replaces the
page). Layout / Repeat Pāli changes (`set_layout` :267, `set_repeat_pali`
:278) are discrete clicks — routing them through the same debounced path is
fine and simpler. Keep the coalescing single-writer: one pending timer,
re-armed on each change.

**Depends on:** nothing (independent of 1.0; together they close finding 1's
risk from both sides).

- [ ] 2.0 Debounce the settings autosave POSTs (finding 4)
  - [x] 2.1 In `src-ts/display_settings.ts`: add a module-level debounce around the POST — e.g. `schedule_post()` (clears + re-arms a ~300 ms `setTimeout` calling `post_settings()`) and `flush_post()` (if a timer is pending: cancel it and POST immediately). Route `on_setting_changed`, `set_layout`, `set_repeat_pali` through `schedule_post()`. *(Implemented as `schedule_post()` + `post_now()` (cancel timer, POST now) + exported `flush_pending_post(keepalive)` (POST only when pending); `post_settings` gained a `keepalive` param.)*
  - [x] 2.2 Wire the flush points: `on_scope_changed` local→default POSTs immediately (bypass/flush the timer — the FR 19 contract); Reset all flushes; add a `pagehide` listener that flushes a pending POST (use `keepalive: true` on that fetch so it survives page teardown). *(Also: default→local flushes a pending POST — that change was made under the default scope; `reset_all` uses `post_now()` so the cancelled timer can't re-post; the `pagehide` listener is registered in `init_display_settings` (sutta pages only); `reset_module_state_for_tests` clears the timer.)*
  - [x] 2.3 Extend `src-ts/display_settings.test.ts` with jest fake timers: a burst of `on_setting_changed()` calls produces exactly one POST after 300 ms; a scope switch mid-burst POSTs immediately and cancels the pending timer; local-scope changes still never POST; `pagehide` flushes. *(Fake timers in both affected describe blocks; existing synchronous-POST assertions updated to advance 300 ms; new tests: 25-tick burst → one POST with the final value, mid-burst scope switch → one immediate POST + cancelled timer, `flush_pending_post(true)` posts once with `keepalive: true` and is a no-op when nothing is pending, reset-all cancels the pending timer.)*
  - [x] 2.4 `npx jest`, `npx webpack`; `make build -B` still clean (no Rust changes expected in this task). *(Jest 74/74, webpack clean, full build clean.)*

### 3.0 Client adopts the server-resolved column list

**Specs.** Root cause of findings 2 + 3: `resolve_sutta_display_options`
(`app_data.rs:341`) silently drops non-segmented columns in Lines mode, but
`/sutta_content_block` responses carry no column state, so
`fetch_content_block` (`content_reload.ts:88`) keeps the client's own list —
per-column-index CSS vars (`--col-N-ink`/`-bg`, the bg gradient in
`display_settings.ts`) land on the wrong rendered columns, the bar shows a
phantom dropdown, and FR 8's "exclude with a notice" has no notice in this
path. **Contract change:** the block response reports the resolved columns in
an `X-SSP-Columns` response header — the JSON array in the same
`{uid, label, author, is_pali}` shape as `SUTTA_DISPLAY.columns`
(`sutta_display_js`, `app_data.rs:556`), percent-encoded (labels like "Pāli"
are non-ASCII; header values must be ASCII-safe). The client **adopts** it,
which also supersedes the client-side Repeat-Pāli arrangement mirroring at
`content_reload.ts:132` (the server's resolved list already carries the
arrangement — remove the mirroring and its "keep in sync" burden). Solo is
already safe: resolution keeps the full column set (only the renderer shows
one), so the header carries the full set. The "+" fix reuses
`option_disabled_reason` (`column_bar.ts:126`) so suggestion and dropdown
enablement can't diverge.

**Depends on:** nothing structurally, but do it after 1.0 so the new
Rust code in the render path follows the locking rule from the start.

- [ ] 3.0 Client adopts the server-resolved column list (findings 2 + 3)
  - [ ] 3.1 In `app_data.rs`: factor the column-list JSON out of `sutta_display_js` into a shared helper (e.g. `display_columns_json(&[Sutta]) -> serde_json::Value` producing the `{uid, label, author, is_pali}` array) used by both `sutta_display_js` and the new header. Expose the resolved column suttas to the route — e.g. `render_sutta_content_block` returns `(String, Vec<...>)` or a small struct, or a sibling fn that resolves columns once and returns both (mind the 1.0 locking rule).
  - [ ] 3.2 In `bridges/src/api.rs` `get_sutta_content_block` (:1583): on success, attach the resolved columns as an `X-SSP-Columns` response header — `serde_json::to_string` then percent-encode the value (switch the route's return type to a Rocket responder that can set a header, e.g. a custom responder or `(Status, Header, RawHtml)` composition). Error paths unchanged.
  - [ ] 3.3 In `src-ts/content_reload.ts` `fetch_content_block`: read `X-SSP-Columns`, `decodeURIComponent` + `JSON.parse`, and set `SUTTA_DISPLAY.columns` to it **before** `reinit_sutta_content()` (so `ds.refresh_columns()` and the bar's `ssp-content-swapped` re-render see the adopted list). Remove the `arrange_display_columns` mirroring block (:132) — the server list is authoritative; keep a fallback (retain current columns, log a warning) if the header is missing/unparsable. Update the comment block that documents the client mirroring.
  - [ ] 3.4 Drop notice (FR 8): in `fetch_content_block`, diff the requested uids against the adopted list; if columns were dropped (Lines-mode non-segmented), show a one-line transient notice (e.g. a small dismissible strip above the column bar or at the top of `#ssp_content`'s parent, styled in `_display_settings.scss`): "*<label>* has no segmented text — shown only in the Columns layout". Auto-dismiss after a few seconds; no notice when nothing was dropped.
  - [ ] 3.5 In `src-ts/column_bar.ts`: make `next_unshown` (:109) layout-aware — pass `layout` (and reuse `option_disabled_reason` with the shown set) so the "+" suggestion skips entries that would be disabled in the dropdowns (non-segmented in Lines mode, already-shown); "+" is disabled when no selectable option remains, not merely when all are shown.
  - [ ] 3.6 Tests: `content_reload.test.ts` — header adoption updates `SUTTA_DISPLAY.columns` (mock fetch with the header), missing-header fallback, drop-notice appears exactly when the adopted list is shorter than the requested one; `column_bar.test.ts` — `next_unshown` skips non-segmented options in Lines mode, still suggests them in Columns mode, returns null → "+" disabled when only disabled options remain. `npx jest`, `npx webpack`.
  - [ ] 3.7 Curl-verify on the live API (port from `api-port.txt`): a Lines-mode block request including a non-segmented column (e.g. mn1 with `columns=...|mn1%2Fen%2Fhorner`) returns `X-SSP-Columns` without the dropped uid; a Columns-mode request returns all; Solo returns the full set; a Repeat-Pāli `alternate` request's header shows the arranged (repeated-Pāli) list. `make build -B`; `cd backend && cargo test`.

### 4.0 Error parity on the full-page sutta routes

**Specs.** `/sutta_content_block` maps a render error containing
"Unknown column sutta uid" (`app_data.rs:460`) to HTTP 404 with the message
(`api.rs:1605`); the full-page routes swallow the same error into a generic
200 "Rendering error" page via the `unwrap_or_else` in
`render_sutta_html_by_uid_with_overrides` (`app_data.rs:735`). Param parity
(FR 13) extends to error behavior. The QML bridge path
(`render_sutta_html_by_uid`, no overrides → no bad columns possible from QML)
keeps returning an error page — only the API routes change their mapping.

**Depends on:** 1.0 (touches the same function; do the locking refactor
first so this lands on the cleaned-up code).

- [ ] 4.0 Error parity on the full-page sutta routes (finding 6)
  - [ ] 4.1 In `app_data.rs`: give the API path access to the render error — e.g. add `try_render_sutta_html_by_uid_with_overrides(...) -> Result<String>` (blank page for empty/unknown sutta uid stays a non-error `Ok`, or a small enum if cleaner) and reimplement the existing infallible fn as a wrapper that maps `Err` to the "Rendering error" page (QML bridge behavior unchanged).
  - [ ] 4.2 In `bridges/src/api.rs` `sutta_html_response` (used by `get_sutta_html_by_uid` and `get_sutta_html_q`): call the `try_` variant; map an error containing "Unknown column sutta uid" to `Status::NotFound` with the message in the body (same match rule as `get_sutta_content_block` :1605 — factor the shared `msg → Status` mapping into a small helper so the two routes can't drift), other errors to 500 with the generic error page.
  - [ ] 4.3 Curl-verify: full-page `?columns=` with a bogus uid → 404 + message on both `get_sutta_html_by_uid` and `get_sutta_html_q`; valid renders unchanged (200, `SUTTA_DISPLAY` present); `/sutta_content_block` behavior unchanged. `make build -B`; `cd backend && cargo test`.

### 5.0 Documentation and housekeeping

**Specs.** Finding 5: the PRD is the feature's reasoning record but §5
Non-Goals still excludes Repeat Pāli and §2/§4 describe a two-mode
`SuttaLayout` — Solo and Repeat Pāli were adopted 2026-07-02 at user request
(they're in the task notes, the feature doc, and AGENTS.md). Amend, don't
rewrite: a dated v2.1 note in §9 Resolved Decisions plus a one-line pointer
at the §5 items it supersedes. Finding 7: the v2 task list's parent `6.0`
checkbox — tick it now that the manual GUI checklist has served its purpose
(it was written for the user in 6.2; leave a note if the user prefers it
open). Docs must also record the contracts introduced by 1.0–4.0.

**Depends on:** 1.0–4.0 (documents their outcomes).

- [ ] 5.0 Documentation and housekeeping (findings 5 + 7)
  - [ ] 5.1 PRD v2.1 amendment: in §9 Resolved Decisions add a dated note — "v2.1 (2026-07-02, recorded 2026-07-03): Solo layout and Repeat Pāli (off/alternate/atend) adopted at user request during implementation, superseding the §5 non-goal and the two-mode `SuttaLayout` wording in §2/§4" — and mark the §5 "No jhana.info Repeat Pāli" bullet with a pointer to that note. Do not rewrite §5's original text (it documents the v2-time decision).
  - [ ] 5.2 Tick the parent `- [ ] 6.0` checkbox in `2026-07-01-192905-tasks-side-by-side-translation-view.md` (all its sub-tasks are done and the manual checklist was delivered to the user in 6.2).
  - [ ] 5.3 Update `docs/sutta-display-settings-and-multi-column-view.md`: the `X-SSP-Columns` resolved-columns response contract and client adoption (replacing the documented client-side Repeat-Pāli mirroring — update that paragraph on both the server and client sides), the drop notice, the layout-aware "+" rule, the autosave debounce + flush points, the settings-cache guard-scoping rule, and the full-page 404 parity.
  - [ ] 5.4 Update `docs/simsapa-localhost-api-search-endpoints.md`: `X-SSP-Columns` on `/sutta_content_block`, and the full-page routes' 404-on-unknown-column behavior. Check `PROJECT_MAP.md` for any new/renamed functions from 1.0–4.0.
  - [ ] 5.5 Final pass: `make build -B`, `cd backend && cargo test`, `npx jest`, `npx webpack`; grep the diff for leftover references to the removed client mirroring (`arrange_display_columns` callers outside `column_bar.ts`/`display_settings.ts`) and confirm the review findings list at the top of this file is fully addressed; tick this file's checkboxes as you go.
