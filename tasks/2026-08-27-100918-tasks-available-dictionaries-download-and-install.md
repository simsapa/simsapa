# Tasks — Available dictionaries: download and install from the Dictionaries window

PRD: [2026-08-27-100918-prd---available-dictionaries-download-and-install.md](./2026-08-27-100918-prd---available-dictionaries-download-and-install.md)

## Relevant Files

- `backend/src/dictionary_catalog.rs` — **new.** The ten-entry catalogue table, the
  pinned series / fallback tag constants, GitHub release-list parsing, tag
  selection, and URL construction. Pure and Qt-free, so all of it is unit-testable
  without a device or a network.
- `backend/src/dictionary_catalog_download.rs` — **new.** Downloads one catalogue
  entry into the dictionaries staging directory, with progress, cancellation and
  a typed error. Split from the catalogue module so the pure half stays
  network-free and its tests stay fast.
- `backend/src/lib.rs` — register both new modules with `pub mod`.
- `backend/src/import_staging.rs` — **reused unchanged.** `staging_dir()`,
  `ensure_free_space()`, `copy_stream_to_file()`, `human_bytes()`,
  `sanitize_staged_file_name()`, `cleanup_staged_file()`, `StagingError`,
  `cancelled_error()`. Read it before writing task 2.0 — most of that task is
  wiring, not new code.
- `backend/src/update_checker.rs` — **reused unchanged.** `to_version()` and
  `compare_versions()` for tag comparison. Do **not** extend
  `get_latest_app_compatible_assets_release()`; it works over Simsapa's own
  releases feed, which does not contain this third-party repo.
- `bridges/src/dictionary_manager.rs` — new invokables, new signals, and a third
  cancel flag alongside `staging_cancel` / `import_cancel`.
- `bridges/assets/qml/com/profoundlabs/simsapa/DictionaryManager.qml` — the
  `qmllint` type stub; every new invokable and signal must be mirrored here.
- `bridges/assets/qml/DictionariesWindow.qml` — the Available section, the new
  download progress frame (`views_stack` index 6), the download run state
  machine, and the hand-off into the existing `start_batch()`.
- `bridges/assets/qml/AvailableDictionaryRow.qml` — **new.** One catalogue row:
  checkbox, name, label, size.
- `bridges/build.rs` — add `"assets/qml/AvailableDictionaryRow.qml"` to `qml_files`.
- `backend/src/dictionary_catalog.rs` (tests module) — unit tests for tag
  selection, catalogue integrity and URL construction.
- `docs/dictionary-import-pipeline.md` — a new section for this second entry
  point into the same import pipeline.
- `PROJECT_MAP.md` — the two new backend modules and the new QML component.

### Notes

- Rust tests: `cd backend && cargo test dictionary_catalog`. All tests in tasks
  1.0 must pass **without network access** — parse fixtures, never live HTTP.
- QML: `make qml-lint` first (it exits 0 on warnings; what matters is a *new*
  warning naming a file you touched), then `make qml-test`.
- Build check: `make build -B`. Do **not** run the GUI as an agent — the
  interactive verification in 6.0 is the user's.
- Every path added to `qml_files` in `bridges/build.rs` must be of the exact form
  `"assets/qml/<Name>.qml"` — relative to `bridges/`, never containing `..`.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, check it off in this file by changing
`- [ ]` to `- [x]`. Update the file after completing each **sub-task**, not just
after an entire parent task.

---

## Tasks

### Specs for 1.0 — catalogue data and tag resolution

Covers FR-11 … FR-17. Depends on nothing; everything else depends on this.

**The catalogue entry** (one `struct`, ten `const` instances or one `const` array):

| field | meaning |
|---|---|
| `label` | asset file name minus `-gd.zip`; also the import label and the row's identity |
| `name` | display name (from the `.ifo` `bookname`, except `abt` — see PRD §4) |
| `lang` | import language code — `pli` / `san` / `si`, **not** derived from the name |
| `entries` | headword count, for display |
| `fallback_size_bytes` | baked-in size, used only when the API lookup failed |

**Constants:** `PINNED_SERIES: (u32, u32) = (1, 0)`, `FALLBACK_TAG = "v1.0.8"`,
`REPO = "digitalpalidictionary/other-dictionaries"`.

**Resolution result** — one struct carrying `tag: String`, `source: TagSource`
(`Api` | `Fallback`), and per-label resolved `url` + `size`. FR-16: sizes and
URLs come from the API response when it succeeded; under the fallback the URL is
built as `https://github.com/<REPO>/releases/download/<tag>/<label>-gd.zip` and
the size is `fallback_size_bytes`.

- [ ] 1.0 Backend: the curated catalogue and release-tag resolution (pure Rust)
  - [ ] 1.1 Create `backend/src/dictionary_catalog.rs` and register it in
        `backend/src/lib.rs`.
  - [ ] 1.2 Define the catalogue entry struct and the ten entries from PRD §4,
        verbatim. Add a comment on `abt` (name deliberately not its `bookname`),
        on `peu` (`pa-en` in the `.ifo` means Pali, so `lang` is `pli`), and on
        `sin-eng-sin` (`si` is absent from `KNOWN_TOKENIZER_LANGS`, so the
        default tokenizer is expected, not a bug).
  - [ ] 1.3 Define `PINNED_SERIES`, `FALLBACK_TAG` and `REPO` as constants, each
        with a comment saying raising the pin is a deliberate code change (PRD
        FR-13 / §11.3) and must not become a setting.
  - [ ] 1.4 Call `update_checker::to_version()` directly on the tag string.
        **Verified: it already strips a leading `v`** (`update_checker.rs:150`,
        `ver.strip_prefix('v').unwrap_or(ver)`, and its doctest asserts
        `to_version("v0.1.0")`). No wrapper helper is needed — do not add one.
  - [ ] 1.5 Write `select_tag(releases: &[ReleaseEntry]) -> Option<String>`:
        skip drafts and prereleases, keep only tags whose major *and* minor equal
        `PINNED_SERIES`, return the highest by `compare_versions()`. A higher
        minor or major must be ignored, not selected (FR-13).
  - [ ] 1.6 Define the serde structs for the GitHub releases JSON — only the
        fields actually used (`tag_name`, `draft`, `prerelease`, `assets[].name`,
        `assets[].size`, `assets[].browser_download_url`). Unknown fields are
        ignored by default; do not model the whole payload.
  - [ ] 1.7 Write `build_fallback_url(label, tag)` and the resolution assembler
        that produces the result struct for either source (FR-16).
  - [ ] 1.8 Write `fetch_releases()` using the existing `reqwest` blocking client
        (already a `backend` dependency; keep it on 0.12), following the shape of
        `update_checker::fetch_releases_info()`. **A `User-Agent` is mandatory,
        not polite** — measured: `api.github.com` answers **403** to a request
        with an empty UA. Set one via `ClientBuilder::user_agent()`.
        Map every failure — offline, DNS, HTTP status, rate limit, unparseable
        JSON — to "fall back", never to an error the user sees while browsing
        (FR-14).
  - [ ] 1.8a Do **not** report a bare 403 as "rate limited". GitHub uses 403 for
        both the missing-UA rejection and the rate limit; only a 403 that also
        carries `X-RateLimit-Remaining: 0` is the rate limit. Both fall back
        either way, so this only affects the log line — but a wrong log line here
        would send the next reader hunting a quota problem that does not exist.
  - [ ] 1.9 Write `resolve_catalogue()`: try `fetch_releases()` + `select_tag()`,
        else the fallback tag. Log the resolved tag once with its source, in the
        form `dictionary_catalog: resolved tag v1.0.8 (source: api)` (FR-17).
  - [ ] 1.10 Cache a **successful** resolution in a process-global for the
        process lifetime, so reopening the window does not spend another of
        GitHub's 60 unauthenticated requests per hour. Do **not** cache a
        failure: a cached fallback would stick for the whole session even after
        the network came back, and reopening the window is exactly how a user
        retries. Do not poll.
  - [ ] 1.10a Note in a comment that GitHub returns releases **newest-first**,
        30 per page by default, and the pinned series currently has 9 releases —
        so page 1 always contains the highest tag and no pagination is needed.
        Revisit only if upstream ever passes 30 releases *above* the pinned
        series.
  - [ ] 1.11 Unit tests, all offline: highest patch in series wins; `v1.1.0` and
        `v2.0.0` are ignored; a draft/prerelease is skipped; an empty list yields
        `None`; a malformed tag is skipped rather than panicking; the fallback URL
        matches the real v1.0.8 URL for `mw`; every catalogue `label` is a valid
        label per `dictionary_manager_core::validate_label()`; every `lang` is
        `pli`, `san` or `si`; there are exactly ten entries and none of them is
        one of the five excluded assets.
  - [ ] 1.12 `cd backend && cargo test dictionary_catalog` passes; `make build -B`
        succeeds. The app is unchanged at this point — nothing calls the module yet.

### Specs for 2.0 — downloading into the staging directory

Covers FR-19, FR-20 (backend half), FR-30, FR-32, FR-33, FR-34.

**Destination:** `import_staging::staging_dir(import_staging::DICTIONARY_FEATURE)`,
file named `<label>-gd.zip` through `sanitize_staged_file_name()`. The constant
**already exists** (`import_staging.rs:129`) and is what the pick path and the
startup sweep both use — never write the string `"dictionaries"` again. This is
load-bearing: `cleanup_staged_file()` decides ownership **by location**, so a
file written anywhere else is never reclaimed, and a feature-string mismatch
fails *silently*.

**Reuse rather than rewrite:** `reqwest::blocking::Response` implements `Read`,
so `import_staging::copy_stream_to_file()` provides chunked copy, the cancel
check between chunks, and deletion of the destination on **every** failure path
— which is FR-32 for free. Follow `asset_manager.rs`'s client shape (connect
timeout, no overall timeout — a 55 MB archive on a slow line must be allowed to
finish).

- [ ] 2.0 Backend: download an archive into the dictionaries staging directory
  - [ ] 2.1 Create `backend/src/dictionary_catalog_download.rs`, register it in
        `backend/src/lib.rs`.
  - [ ] 2.2 Write `download_entry(label, url, expected_bytes, cancel, progress)
        -> Result<PathBuf, StagingError>`: resolve the destination via
        `staging_dir(DICTIONARY_FEATURE)` + `sanitize_staged_file_name()`, create
        the directory, and call `ensure_free_space()` with the expected size
        before opening the connection. (`ensure_free_space` takes
        `Option<u64>`, walks up to the nearest existing directory, and passes
        when the volume figure is unreadable — no extra guarding needed.)
  - [ ] 2.3 Build the HTTP client with a connect timeout and no overall timeout;
        `GET` the asset URL. **Redirects need no configuration** — verified:
        reqwest 0.12's default policy is `Policy::limited(10)`
        (`redirect.rs:161`), which covers the GitHub → CDN hop. Say so in one
        comment so nobody adds a redundant `.redirect(...)` later.
  - [ ] 2.4 Check the HTTP status **before** streaming. Map `404` to a distinct
        error code (`asset_not_found`) whose message names the resolved tag and
        the asset file name (FR-31); map other non-2xx to `http_status`.
  - [ ] 2.5 Stream the body through `copy_stream_to_file()` with the cancel flag
        and the progress callback. Prefer `Content-Length` for the total; fall
        back to the catalogue's `fallback_size_bytes` so the progress bar is
        determinate either way.
  - [ ] 2.6 After the copy, reject a zero-byte or implausibly short result via
        `reject_empty()`, deleting the file. A truncated archive must never reach
        the importer (FR-32).
  - [ ] 2.7 Throttle progress callbacks to ~100 ms, matching `import_staging`'s
        existing behaviour, and use `f64` byte counts at the boundary that will
        cross into QML.
  - [ ] 2.8 Give every error a `code` and a `step` so a failure can never be
        unattributed, and make sure `code` is what callers match on — never the
        message text. Reuse `cancelled_error()` for a user cancel so it travels
        the same channel and is distinguished by `code == "cancelled"`.
  - [ ] 2.9 No new sweep. **Verified:** `init_app_data()` already calls
        `sweep_orphaned_staged_files(DICTIONARY_FEATURE)` (`backend/src/lib.rs:278`),
        age-gated at an hour, treating an unreadable timestamp as "too young" —
        so an archive orphaned by a crash mid-download is already reclaimed on a
        later launch, *provided* 2.2 puts it in the staging directory. Record
        that dependency in a comment.
  - [ ] 2.10 Unit-test what can be tested offline: destination path construction,
        the free-space pre-check, and the error `code`/`step` for each branch.
        The network path itself is covered by the manual run in 6.0.
  - [ ] 2.11 `cargo test` and `make build -B` pass.

### Specs for 3.0 — the bridge surface

Covers FR-11 … FR-17 (exposure) and FR-18 (dispatch).

**Proposed API** — settle the exact names here, because task 4.0 and the
`qmllint` stub both encode them:

```
// invokables
available_dictionaries() -> QString        // JSON; may block briefly on first
                                           // call, so call it from a worker and
                                           // deliver via a signal (see 3.3)
refresh_available_dictionaries()           // async; emits availableDictionariesReady
download_available(labels: QStringList) -> QString   // "ok" or an error message
abort_available_download()

// signals
availableDictionariesReady(items_json: QString)
availableDownloadProgress(label: QString, done_bytes: f64, total_bytes: f64)
availableDownloadFinished(label: QString, path: QString)
availableDownloadFailed(label: QString, message: QString)
```

**The JSON payload** carries the resolution as well as the rows, so FR-8's
source line has something to bind to:
`{ "repo": "...", "tag": "v1.0.8", "tag_source": "api", "items": [ { label, name,
lang, entries, size_bytes, size_text, url, size_is_approximate } ] }`.

**Non-negotiables in this file:** every `qt_thread.queue()` goes through
`crate::queue_or_log()` — never `.unwrap()`, never `let _ =` — because this
window is destroyed on close and `ObjectDestroyed` is a live path; and log and
continue, never log and return, so cleanup still runs.

- [ ] 3.0 Bridge: expose the catalogue and the download run on `DictionaryManager`
  - [ ] 3.1 Add a third cancel flag `download_cancel: Arc<AtomicBool>` to
        `DictionaryManagerRust`, next to `staging_cancel` and `import_cancel`,
        with a comment saying why it is separate (a third stage with a third
        cancel button; one shared flag would be set by the wrong screen).
  - [ ] 3.2 Declare the invokables and signals above in the `#[cxx_qt::bridge]`
        block, with `#[cxx_name = "..."]` camelCase signal names matching the
        existing convention.
  - [ ] 3.2a Add the `QStringList` type to the bridge's `unsafe extern "C++"`
        block — `include!("cxx-qt-lib/qstringlist.h")` +
        `type QStringList = cxx_qt_lib::QStringList;`. `dictionary_manager.rs`
        currently declares only `QString` and `QUrl`; `asset_manager.rs:27` is
        the worked example. Without it `download_available(labels: QStringList)`
        does not compile.
  - [ ] 3.3 Implement `refresh_available_dictionaries()`: spawn a thread, call
        `dictionary_catalog::resolve_catalogue()`, serialise, emit
        `availableDictionariesReady`. The network lookup must never run on the
        GUI thread.
  - [ ] 3.4 Implement `download_available(labels)`: reset `download_cancel`,
        spawn one worker that walks the labels **in catalogue order** (FR-18),
        emitting progress / finished / failed per label, and continues past a
        failure rather than returning (FR-27).
  - [ ] 3.5 Implement `abort_available_download()` — set the flag only; the
        worker observes it between chunks.
  - [ ] 3.6 Format sizes for display with `import_staging::human_bytes()` so the
        list and the progress frame agree on wording.
  - [ ] 3.7 Mirror every new invokable and signal in
        `bridges/assets/qml/com/profoundlabs/simsapa/DictionaryManager.qml` with
        the correct signature and a trivial return value. `qmllint` needs this;
        omitting it produces a runtime type failure with a green build.
  - [ ] 3.8 `make build -B` and `make qml-lint` pass; no new warning names
        `DictionaryManager.qml`.

### Specs for 4.0 — the Available section

Covers FR-1 … FR-10. Depends on 3.0 for its data; needs nothing from 2.0, so the
list can be seen working before any download code is exercised.

**Placement:** inside the existing `ScrollView` in the idx-0 list frame, below
the `Repeater` over `root.user_dictionaries`, so both lists scroll together
(FR-9). No collapse control.

**State:** `property var available_items: []`, `property var checked_labels: []`,
`property string catalogue_tag: ""`, `property string catalogue_repo: ""`,
`property string catalogue_tag_source: ""`.

**The hide rule (FR-6)** is a filter, not a flag: an entry whose `label` appears
in `root.user_dictionaries` is not rendered. Because `refresh_list()` is already
called after every import, delete and rename, driving the filter off
`user_dictionaries` makes FR-6's "disappears on import, reappears on delete"
automatic — do **not** add a parallel refresh path.

- [ ] 4.0 QML: the Available section in the Dictionaries window
  - [ ] 4.1 Create `bridges/assets/qml/AvailableDictionaryRow.qml` — a checkbox,
        the name (wrapping), the label, and the size. Follow the existing split:
        the **delegate** in `DictionariesWindow.qml` declares
        `required property var modelData` and assigns plain typed properties down
        into the row component; `DictionaryListItem.qml` itself takes ordinary
        properties and knows nothing about `modelData`. Mirror that.
  - [ ] 4.2 Add `"assets/qml/AvailableDictionaryRow.qml"` to `qml_files` in
        `bridges/build.rs`, in the exact `"assets/qml/<Name>.qml"` form.
  - [ ] 4.3 Add the state properties above to `DictionariesWindow.qml` and a
        `Logger { id: logger }` usage for any diagnostics (single concatenated
        string argument — never `console.*`, never comma-separated arguments).
  - [ ] 4.4 Call `refresh_available_dictionaries()` from `Component.onCompleted`
        and handle `onAvailableDictionariesReady` by parsing the JSON into
        `available_items` (inside a `try`/`catch`, logging a parse failure the
        way `refresh_list()` does).
  - [ ] 4.5 Add the **Available** section header and a `Repeater` over the
        filtered items, below the imported-dictionaries `Repeater` (FR-1, FR-6).
  - [ ] 4.6 Render the source line: `<repo> <tag>`, e.g.
        `digitalpalidictionary/other-dictionaries v1.0.8` (FR-8). It must render
        something sensible before the resolution arrives and update when it does.
  - [ ] 4.7 Add the **Download and Import** button below the list, disabled while
        `checked_labels` is empty (FR-4), plus the combined-size label for the
        checked set (FR-5).
  - [ ] 4.8 Mark sizes as approximate (e.g. `~55 MB`) when `size_is_approximate`
        is set, i.e. when the tag came from the fallback (FR-16).
  - [ ] 4.9 Replace the "No imported dictionaries yet" paragraph with the
        always-visible one-line link to the releases page, placed under the
        Available list (FR-7).
  - [ ] 4.10 Check the layout at phone width — name wrapping, checkbox and size
        legible, no horizontal overflow (FR-10). Any new `Dialog` added here must
        follow the width-clamp rule and, if titled with wrapping text, use
        `header: DialogHeader { … }`.
  - [ ] 4.11 `make qml-lint` and `make build -B` pass. At this point the list is
        visible and the button does nothing yet.

### Specs for 5.0 — the download run

Covers FR-18, FR-21 … FR-26. Depends on 2.0, 3.0 and 4.0.

**The frame is `views_stack` index 6 — appended, not inserted.** Indices 0–5
(list / delete / import / rename / summary / error) are hard-coded at roughly
fifteen call sites; inserting in the middle silently reroutes all of them.

**The hand-off (FR-21):** each successful download appends
`{ kind: "zip", path: <staged path>, member: "", label, lang }` to a queue. When
the last download finishes, hand the whole queue to the existing
`start_batch(items)`. `member` is `""` for every entry and `scan_source()` is
never called (FR-22) — the label, language and member are known from the
catalogue, which is the entire point of the feature.

**Keep-screen-on (FR-25):** holder name `dictionary-download-batch`, distinct
from the existing `dictionary-import-batch` / `-staging` / `-scan`. Acquire when
the run starts; release in the **one** function every ending passes through.
Releasing a name you do not hold is a logged no-op, not a theft — but an
unreleased hold survives the window.

**Two collisions with the existing batch code, found by reading it.** Both are
silent, and both must be settled here rather than discovered at 5.10:

1. **`start_batch()` resets `batch_failed = []`** (`DictionariesWindow.qml:143`).
   Any download-phase failure recorded before the hand-off is therefore erased
   the moment the import phase starts. Keep download failures in their **own**
   `download_failed` list, and have the summary frame render both lists — do not
   push them into `batch_failed`.
2. **`finish_batch()` sets `op_kind = "import_batch"`** (`:205`), overwriting
   whatever the download run set. So a run that reaches the import phase always
   ends under the import title. Decide explicitly: either have `finish_batch()`
   leave `op_kind` alone when a download run is active, or accept
   `"import_batch"` as the terminal kind and have the summary include the
   download failures whenever `download_failed` is non-empty. **Prefer the
   second** — it adds no branch to a function four other flows depend on.

- [ ] 5.0 QML: the download run — progress frame and hand-off to the import batch
  - [ ] 5.1 Add the download progress frame as `views_stack` index 6: current
        dictionary name, bytes done / total, `n of m` position, and a Cancel
        button (FR-20).
  - [ ] 5.2 Add the run's state properties (`download_queue`, `download_index`,
        `download_total`, `download_active`, `download_cancelled`,
        `pending_import_items`, `download_failed` list).
  - [ ] 5.3 Write `start_download_run(labels)`: sort the labels into catalogue
        order, acquire `dictionary-download-batch`, switch to frame 6, and call
        `download_available()`.
  - [ ] 5.4 Handle `onAvailableDownloadProgress` — update the bytes and the
        current name; ignore ticks once a cancel is pending, matching the
        existing `import_aborting` guard.
  - [ ] 5.5 Handle `onAvailableDownloadFinished` — append the item to
        `pending_import_items` in the shape above.
  - [ ] 5.6 Handle `onAvailableDownloadFailed` — record the failure with its
        message and continue; do not abort the run (FR-27).
  - [ ] 5.7 When the last label is done, call `finish_download_phase()`: hand
        `pending_import_items` to the existing `start_batch()` **first**, then
        release `dictionary-download-batch`. That order matters — `start_batch()`
        acquires `dictionary-import-batch`, so releasing first leaves an instant
        with no holder at all. If the list is empty (everything failed), release
        the holder and go straight to the summary frame instead.
  - [ ] 5.8 Wire Cancel to `abort_available_download()` for the download phase and
        to the existing `abort_import()` once the import phase has started
        (FR-24). Cancelling must stop before the next item starts, not mid-batch
        by killing the worker.
  - [ ] 5.9 Extend the `onClosing` guard to refuse a close while frame 6 is
        current, alongside the existing 1/2/3 check (FR-26).
  - [ ] 5.10 Extend the shared summary frame to report the download phase:
        add an `op_kind` `"download_batch"` for the everything-failed ending
        (which never reaches `start_batch()`), and for the normal ending append
        the `download_failed` entries to the existing `"import_batch"` body
        whenever that list is non-empty. Do not add a branch to `finish_batch()`
        — see the two collisions in the specs above.
  - [ ] 5.11 Verify the keep-screen-on holder is released on **all three**
        endings — success, all-failed, and cancel — by inspection of the code
        paths, since every ending must pass through one function.
  - [ ] 5.12 `make qml-lint`, `make qml-test` and `make build -B` pass.

### Specs for 6.0 — failures, cleanup, verification

Covers FR-27 … FR-34 and PRD §9.

**The cleanup is already written** — `finish_batch()` calls
`cleanup_staged_file(item.path)` for every queue item on every ending. The work
here is proving it fires for downloaded archives too, including for items that
failed *import* (they are in the batch queue) and for items that failed
*download* (they are not — `copy_stream_to_file` already deleted them).

- [ ] 6.0 Failures, cleanup, and end-to-end verification
  - [ ] 6.1 Make download failures textually distinguishable from import failures
        in the summary (FR-29) — the user needs to know whether to check their
        connection or report a bad archive.
  - [ ] 6.2 Check the 404 message names the resolved tag and asset (FR-31), and
        that it reaches `log.txt`, since this is the symptom of an upstream
        rename and must be diagnosable from a user's log without a reproduction.
  - [ ] 6.3 Confirm a failed entry is still offered in Available afterwards
        (FR-28) — it follows from the FR-6 filter, but verify it rather than
        assume it.
  - [ ] 6.3a Confirm FR-6 is the **only** duplicate handling needed: import a
        dictionary as `cone`, reopen the window, and check the `cone` row is gone
        from Available — which is what makes a same-label collision unreachable
        from this flow. No per-item collision handling is required here. A
        different-label duplicate (`cone-gd` imported manually, then `cone`
        downloaded) is accepted behaviour per PRD §6 — the user sees both and
        deletes one. Do not add a content-matching check.
  - [ ] 6.4 Verify no archive survives a run: after success, after a mid-run
        cancel, and after a forced failure. Check
        `staging_dir("dictionaries")` is empty each time (FR-33).
  - [ ] 6.5 Grep for the staging feature string and confirm exactly one constant
        is used by both the pick path and the download path (FR-34).
  - [ ] 6.6 Full check: `cd backend && cargo test`, `make qml-test`,
        `make build -B`.
  - [ ] 6.7 **Hand to the user for the interactive runs** (agents do not run the
        GUI): install `nyanatiloka` (0.20 MB) and `whitney` (0.16 MB) as the cheap
        end-to-end case; `cone` (55.69 MB) for progress display and
        keep-screen-on; `sin-eng-sin` for the unknown-tokenizer path, which must
        import successfully with the default tokenizer rather than fail.
  - [ ] 6.8 **Hand to the user:** the airplane-mode run — the Available list must
        still render all ten entries from the fallback tag, and pressing the
        button must produce a named per-entry download error rather than a hang
        or a blank frame (PRD §9.2).
  - [ ] 6.9 If a device run is possible, check the Android path: the section at
        phone width, and the keep-screen-on holder released after the run.

### Specs for 7.0 — documentation

- [ ] 7.0 Documentation
  - [ ] 7.1 Add a section to `docs/dictionary-import-pipeline.md` covering this
        second entry point into the same pipeline: the catalogue, the tag
        resolution and its fallback, the download → staging → `import_zip` route
        that skips `scan_source` entirely, and the three staging rules it
        inherits. Cross-link `docs/releases-info-and-fallback.md` with the
        explicit note that this is a *different* mechanism from the app's own
        release check.
  - [ ] 7.2 Record the traps a future reader would otherwise re-derive: `abt` is
        the CPED and its display name is deliberately not its `bookname`; `peu`'s
        `pa-en` means Pali; `si` is not a known tokenizer language; the new frame
        is `views_stack` index 6 because the earlier indices are hard-coded; and
        raising `PINNED_SERIES` is a code change because a minor/major bump is
        exactly when the import mechanism itself is likely to need work.
  - [ ] 7.3 Update `PROJECT_MAP.md` with the two new backend modules and the new
        QML component.
  - [ ] 7.4 Follow the docs convention: describe current behaviour and the traps,
        not the history of how it was built. Measurements belong in the commit
        message.
