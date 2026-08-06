# PRD — Fulltext search on storage volumes that do not support `flock()` (SD cards)

**Date:** 2026-08-05
**Status:** Draft — **blocked on phase 1**, not yet implemented
**Phase:** 2 of 2. Phase 1 is
`2026-08-05-201545-prd---run-storage-diagnostics.md`, which ships a
behaviour-neutral **"Run Storage Diagnostics"** action that measures the
assumptions this PRD rests on — including running the candidate wrapper
end-to-end on an affected device.

**Do not begin implementing this PRD until phase 1 has returned data from a
real affected device.** Phase 1's §9 decision gate says what each outcome means;
in particular, if `mmap` does not work on those volumes, the wrapper designed
here is necessary but **not sufficient**, and §4.1 needs redesigning before any
of it is written. Phase 1 also *implements* the probes (FR-8, FR-34..36) and the
wrapper (FR-1..FR-16) in their final locations, wired only into the diagnostic —
so phase 2 is largely a matter of switching the real call sites over.

## 1. Introduction / Overview

When a user chooses an **SD card** as the app's storage location in the initial
`StorageDialog`, setup completes successfully, the SQLite databases work, and
**Database Validation reports no errors** — but **every fulltext search returns
zero results, silently**. ContainsMatch searches still work.

The cause is not a path or permission problem, and not a missing file. It is a
**file-locking primitive that the SD card's filesystem does not implement**:

- Tantivy's `MmapDirectory::acquire_lock` (`tantivy-0.25.0/src/directory/mmap_directory.rs:476`)
  calls `file.lock_exclusive()` / `try_lock_exclusive()` — i.e. **`flock(2)`**.
- Android mounts a *portable* (non-adopted) SD card at `/storage/<UUID>/…` through
  a **FUSE layer over exFAT/FAT32**, which does **not implement `flock`**. The call
  returns **`ENOSYS` (errno 38, "Function not implemented")**.
- `IndexReader` construction acquires `META_LOCK` **unconditionally**
  (`tantivy-0.25.0/src/reader/mod.rs:194`), so **`index.reader()` fails for every
  index**, for reading as well as writing.

Evidence from a ChromeOS user's log
(`feedback-and-bug-reports/log-chromebook.txt`), storage path
`/storage/E297186276AA7E917DC8E6AC2FFA3BF32E0D48BB/Android/data/io.github.simsapa.app/files`:

```
WARN: Failed to open index at …/app-assets/index/suttas/pli: Failed to acquire Lockfile: IoError(Os { code: 38, kind: Unsupported, message: "Function not implemented" }). None
WARN: Failed to open index at …/app-assets/index/suttas/en:  … (same)
WARN: Failed to open index at …/app-assets/index/suttas/san: … (same)
WARN: Failed to open index at …/app-assets/index/dict_words/pli: … (same)
WARN: Failed to open index at …/app-assets/index/dict_words/en:  … (same)
WARN: Failed to open index at …/app-assets/index/library/en: … (same)
INFO: FulltextSearcher opened: 0 sutta language indexes, 0 dict language indexes, 0 library language indexes
INFO: Fulltext searcher initialized
```

Note the last two lines: **zero indexes opened, and the app then reports the
searcher as "initialized".** Searches against an empty index map return an empty
result set, which the UI renders identically to "no matches found".

**Why SQLite is unaffected, and why this is invisible to every existing check:**
SQLite uses **POSIX `fcntl` record locks**, which *do* work on this mount.
Tantivy uses **`flock`**, which does not. The existing tier-2 storage probe
(`backend/src/storage_probe.rs`) writes and exercises a real SQLite database —
so the card passes the probe, passes Database Validation, and passes every
first-run check, while fulltext search is completely broken.

**Goal:** make fulltext search (and index writing) work on storage volumes that
do not implement `flock`, and make any remaining index-open failure *visible*
instead of silently indistinguishable from "no results".

## 2. Goals

1. Fulltext search (Suttas, Dictionary, Library) returns correct results when the
   storage location is an SD card or any other volume whose filesystem does not
   implement `flock`.
2. Index **writing** paths (search-index rebuild, StarDict/dictionary import and
   its index reconcile, Library book import) work on those same volumes.
3. Affected users are fixed by **updating the app alone** — no re-download of the
   ~600 MB index, no moving of data, no re-running setup.
4. A failure to open a fulltext index is **surfaced to the user** in two places:
   the search results area and the Database Validation dialog.
5. No regression, behaviour change, or performance cost on ordinary filesystems
   (ext4, F2FS, APFS, NTFS) where `flock` works normally.

## 3. User Stories

- **As a user with a phone that has little internal storage**, I install the
  databases to my SD card so the app fits, and fulltext search works the same as
  it would on internal storage.
- **As a user whose fulltext search returns nothing**, I see a message telling me
  the search index could not be opened — rather than an empty result list that
  looks like my query simply had no matches.
- **As a user who reports the problem**, the Database Validation dialog shows me a
  clear "Fulltext index" row with the failure, so I can send a meaningful report.
- **As a user already in this broken state**, I update the app, restart it, and
  search works — I do not have to download 600 MB of index files again.
- **As a maintainer triaging a bug report**, the log distinguishes "the index
  directory is missing" from "the index exists but could not be locked".

## 4. Functional Requirements

### 4.1 The lenient-lock directory (the fix)

1. The system must provide a custom Tantivy `Directory` implementation — working
   name `LenientLockMmapDirectory` — that wraps `MmapDirectory` and **delegates
   every trait method to the inner directory unchanged**, with the single
   exception of `acquire_lock`.
2. `acquire_lock` must first attempt the inner `MmapDirectory::acquire_lock`.
3. If the inner call **succeeds**, the returned `DirectoryLock` must be handed
   back unchanged. On a normal filesystem the behaviour is therefore bit-for-bit
   identical to today.
4. If the inner call fails with **`LockError::IoError`** (which is what an
   `ENOSYS`/`EOPNOTSUPP`/`ENOTSUP` from `flock` produces on the *blocking*
   `META_LOCK` path), the system must fall back to a **process-internal lock**:
   acquire a process-global mutex keyed by the **absolute lock file path**, and
   return a `DirectoryLock` whose guard releases that mutex on drop.
    - **FR-4a.** `IoError` has a **third source** that the fallback must not silently absorb:
      `MmapDirectory::acquire_lock` opens the lock file *before* locking it, so a
      read-only or otherwise unwritable directory fails at `open_write` and produces
      `LockError::IoError` too. Falling back there converts a genuinely broken volume
      into an apparently successful lock. The classified errno must therefore be
      retained and **logged** at the fallback, distinguishing "fell back after an
      unsupported-operation errno" from "fell back after some other `IoError`" — the
      latter is a fault to surface (FR-19..FR-22), not a workaround to celebrate.
      Phase 1's section E reports the same three-way distinction (diagnostics PRD
      FR-27a).
5. If the inner call fails with **`LockError::LockBusy`**, the system must
   distinguish two cases, because `MmapDirectory` maps *all* errors from the
   non-blocking `try_lock_exclusive` path (used by `INDEX_WRITER_LOCK`) to
   `LockBusy`, conflating "another process holds it" with "not supported":
   - If the filesystem is known-unsupported (see FR-7), fall back to the
     process-internal lock as in FR-4.
   - Otherwise, propagate `LockBusy` unchanged (a genuine contention error).
6. The process-internal lock table must be keyed on the **canonicalised absolute
   path** of the lock file, so that two `Directory` instances opened on the same
   index directory share one mutex. It must not be keyed on the relative
   `.tantivy-meta.lock` / `.tantivy-writer.lock` name alone, which is identical
   across all index directories.
    - **FR-6a.** `canonicalize()` **can fail on exactly the volumes this PRD targets** — it is
      avoided elsewhere in the storage code for that reason (`same_path()` never
      canonicalises; see `docs/relocated-storage-recovery.md`). The key derivation
      must therefore have a defined fallback: on failure, use the absolutised path
      as-is. It must **never** skip or drop the entry — that would hand two
      `Directory` instances on one index directory two different mutexes, which is
      the precise failure FR-6 exists to prevent. The same normalisation helper must
      serve both this table and the FR-7 support cache, so the two cannot disagree
      about what "the same directory" means.
7. The system must determine "does this directory support `flock`" **once per
   index directory**, by performing the probe described in FR-8, and cache the
   answer for the lifetime of the process. It must not re-probe on every lock
   acquisition.
8. The `flock` support probe must: create (or open) a small file inside the index
   directory, attempt `try_lock_exclusive()` on it, classify `ENOSYS` /
   `EOPNOTSUPP` / `ENOTSUP` / `EINVAL` as *unsupported*, release the lock, and
   delete the probe file on every exit path (success, failure and panic), in the
   same style as `storage_probe.rs`'s `Drop` guard. It must never leave litter on
   the user's card. Two implementation notes: `EOPNOTSUPP` and `ENOTSUP` are the
   **same number (95)** on Linux and Android, so the classification cannot
   distinguish them; and `fs4 0.13` is rustix-based and exports no errno
   constants, so `libc` supplies them. (This probe is written in phase 1 —
   diagnostics PRD FR-17 — and adopted here unchanged.)
9. Every existing call site that opens a Tantivy index must use the new
   directory instead of a bare `MmapDirectory`:
   - `backend/src/search/searcher.rs:136` (`open_single_index`)
   - `backend/src/search/indexer.rs:34` (`open_or_create_index`, which serves all
     six index builders and the writer paths)
   - `backend/src/search/indexer.rs:764` (dictionary index deletion)
   - `backend/src/search/indexer.rs:814` (`list_indexed_source_uids_in_dict_index`)
10. Because the fallback lock is **process-internal only**, the system must state
    and rely on the invariant that **only one Simsapa process ever touches an
    index directory at a time**. This is already true (the searcher is the
    process-global `FULLTEXT_SEARCHER` in `backend/src/lib.rs:169`, and the
    embedded webserver shares it). The invariant must be recorded as a comment at
    the fallback, so a future multi-process design does not silently inherit an
    unsafe lock.
11. The read path (`FulltextSearcher::open_single_index`) must use
    `Index::open` rather than `Index::open_or_create` when the directory already
    contains an index. Creating an index from the *search* path is never correct.
    **This is hygiene, not part of the fix:** `Index::open_or_create` takes no
    lock at all (see §7), and the log confirms it succeeded on the affected
    device. (`Index::open_or_create` must remain in the indexer's write paths.)
12. The fallback lock guard must be a **hand-rolled `Send + Sync` type** (e.g. a
    struct holding an `Arc` over a `Mutex`/`Condvar` slot, releasing in `Drop`).
    It must **not** be a `MutexGuard`: `DirectoryLock` is
    `Box<dyn Send + Sync + 'static>` (`directory/directory.rs:43,50`) and both
    `std::sync::MutexGuard` and `parking_lot::MutexGuard` are `!Send`.
13. The fallback must be a **real mutual exclusion**, never a no-op guard. The
    garbage collector runs automatically after every commit and every merge
    (`indexer/segment_updater.rs:451,674`) and takes `META_LOCK`
    (`managed_directory.rs:138`) precisely to avoid deleting segment files a
    reader is opening — and this app keeps the previous searcher alive while a
    rebuild or import runs, calling `reinit_fulltext_searcher()` only afterwards.
    The race is therefore real in-process.
14. The fallback must **preserve each lock's blocking semantics**: `META_LOCK`
    (`is_blocking: true`) blocks; `INDEX_WRITER_LOCK` (`is_blocking: false`)
    must `try_lock` and return `LockBusy` on contention, so the single-writer
    guarantee still holds in-process.
15. The blocking fallback must use a **bounded retry loop** (mirroring tantivy's
    own `RetryPolicy { num_retries: 100, wait_in_ms: 100 }`,
    `directory/directory.rs:89`) and log an error on exhaustion, rather than
    blocking unboundedly. Tantivy does not currently nest `META_LOCK`
    acquisitions, but that is an upstream detail; a bounded wait fails loudly
    instead of freezing the UI thread if a future version does.
16. Once the probe has classified a directory as **unsupported**, the wrapper
    must skip the inner `acquire_lock` entirely rather than calling it and
    discarding the error. This avoids a failing syscall on every reader reload,
    and avoids creating a `.tantivy-meta.lock` file that can never serve its
    purpose on that volume (`MmapDirectory` creates lock files and never deletes
    them — `ReleaseLockFile`'s `Drop` only closes the fd).

### 4.1b Reader reload policy

17. `FulltextSearcher::open_single_index` must build its reader with
    **`ReloadPolicy::Manual`** instead of the default `OnCommitWithDelay`.
    The default spawns a **polling thread per index** that re-reads and CRC32s
    `meta.json` every 500 ms for the life of the process
    (`directory/file_watcher.rs:12,47-62`) — with six indexes open that is six
    threads and ~12 file reads per second against the SD card, growing with every
    downloaded language. The app does not need it, because every index mutation
    is already followed by an explicit `reinit_fulltext_searcher()`.
18. This is an **independent improvement, not an alternative fix**:
    `open_segment_readers` takes `META_LOCK` regardless of reload policy, so the
    wrapper is still required.

### 4.2 Honest readiness reporting

19. `FulltextSearcher` must expose the number of indexes it opened and the list of
    per-directory open failures (path + error string). The failure list must be
    **cleared at the top of `FulltextSearcher::open()`**, so a failure recorded
    before a storage recovery or an index rebuild is not still reported
    afterwards. (Phase 1 adds this global and its clear-on-reopen rule —
    diagnostics PRD FR-31.)
20. `is_fulltext_searcher_ready()` (`backend/src/lib.rs:358`) must not report
    `true` when **zero** indexes are open. Today it reports `true` merely because
    `FulltextSearcher::open()` returned `Ok` with empty maps — which is what makes
    the `/health` route's `fulltext_searcher_ready` field misleading.
21. `reinit_fulltext_searcher()` must log at **ERROR** (not INFO) when it
    completes with zero indexes open, and the "Fulltext searcher initialized"
    message must include the counts so a log alone tells the story.
22. Per-index-directory open failures must be recorded in a process-global report
    (mirroring the existing `StartupDbReport` pattern in `backend/src/db/mod.rs`)
    so both the search UI and Database Validation can read the same authoritative
    data rather than re-probing.

### 4.3 Surfacing the failure in the search UI

23. When a **FulltextMatch** (or Combined, on the paths that use the Tantivy
    index) search runs and **no index is open for the areas being searched**, the
    results area must display an explanatory message instead of an empty list.
24. The message must clearly distinguish the two states: *"No results found"*
    versus *"The fulltext search index could not be opened."*
25. The message must name the reason from the recorded failure where one exists
    (e.g. that the storage location does not support the file locking the search
    index requires) and point the user to **Database Validation** for details.
26. This must not fire for the ordinary case where a language index genuinely does
    not exist because the user has not downloaded that language.

### 4.4 Surfacing the failure in Database Validation

27. The Database Validation dialog (`assets/qml/DatabaseValidationDialog.qml`)
    must gain a **"Fulltext index"** result row alongside the existing Appdata /
    DPD / Dictionaries rows, driven by the same
    `database_validation_result(database_name, is_valid, message)` signal
    (`bridges/src/sutta_bridge.rs:759`) so no new signal plumbing is needed.
28. The row must be **invalid** when the index directory exists but zero indexes
    could be opened, and the message must state the underlying error (including
    the `flock`-unsupported case in plain language).
29. The row must be **valid** when at least one index per expected area opened,
    and must report the counts (e.g. "3 sutta, 2 dictionary, 1 library index").
30. The row must report a distinct, honest message when the index directory is
    **absent** ("Fulltext index files not found") rather than conflating that with
    a lock failure — the same principle the `StartupDbReport` established for
    missing database files.

### 4.5 Diagnostics for the storage location

31. The tier-2 storage probe (`backend/src/storage_probe.rs`) must additionally
    test `flock` support and **record** the result. Its documented contract of
    "demote only, never promote" is unchanged.
32. A volume that fails **only** the `flock` test must **not** be demoted to
    unusable, because with FR-1..FR-11 in place the app works on it. The result is
    recorded for diagnostics and logging only.
33. The startup log must include the `flock` support verdict for the resolved
    storage path, next to the existing `storage_path` diagnostic.

### 4.6 The `mmap` probe — must be implemented and released FIRST

34. The probe of FR-8 must **also** attempt a read-only `mmap` of a small file in
    the index directory, classify the result, and record it alongside the `flock`
    verdict. Both verdicts must appear in the startup log (FR-33).
35. **This probe must be implemented and shipped before, or at the same time as,
    the wrapper — and its result reviewed before the wrapper is considered
    sufficient.** Every index read goes through `memmap2::Mmap::map()`
    (`MmapDirectory::get_file_handle` → `open_mmap`). FUSE volumes mounted with
    `direct_io` reject `mmap(MAP_SHARED)` with `ENODEV`. We have **never observed
    a successful index read from an SD card**, because the lock failure
    short-circuits everything after it — and SQLite working proves nothing, since
    Android SQLite defaults to `mmap_size = 0` and uses `pread`/`pwrite`.
36. If the probe reports `mmap` unsupported on affected devices, the wrapper is
    **necessary but not sufficient**, and a follow-up PRD is required for a
    non-mmap `Directory` (a `pread`-backed or read-into-RAM `FileHandle`). That
    is a substantially larger job and must not be discovered after the wrapper
    ships.

### 4.7 SD card performance notice at selection time

An SD card is slower than internal storage, and fulltext search is the most
I/O-heavy thing the app does — it reads memory-mapped index segments spread
across a ~600 MB index. A user who chooses a card should be told to expect
slower searches **before** committing to a ~1 GB download, not left to wonder
whether the app is broken. This also matters for triage: without the notice, a
"searches are slow" report is indistinguishable from a bug.

37. When the user selects a **removable** volume in `StorageDialog.qml` and
    presses **Select**, a confirmation dialog must appear before the choice is
    recorded, explaining that searches will be slower on a memory card than on
    internal storage.
38. The notice must be gated on the volume being **removable**, using the
    `is_removable` field the tier-1 scan already produces (see
    `backend/src/lib.rs:933` and `StorageCandidatesList.qml`). It must **not**
    fire for internal storage, and must not be inferred from the path string.
39. The dialog must offer **two** actions — continue with the card, or go back and
    choose differently — with continuing as an explicit press, not the default
    dismissal. Cancelling must return to the list with the selection intact and
    **nothing recorded**.
40. The notice must **not block** the choice. It informs; the user decides. An SD
    card is a legitimate and often necessary choice on a device with little
    internal storage.
41. The same notice must appear on the equivalent selection paths in
    `StorageRecoveryWindow.qml`, which shares `StorageCandidatesList.qml`, so the
    behaviour does not depend on which screen the user reached the card from.
42. When `auto_select_single_location()` (`StorageDialog.qml:95`) picks a
    **removable** volume without asking — because it is the only usable location —
    the notice must still be shown, as an **informational** dialog with a single
    **Continue** action. There is no choice to offer, but the expectation still
    needs setting before the download. It must not be turned back into a
    question with one answer.
43. The wording must be plain and non-alarming: searches will take longer,
    everything else works normally, and the card can be changed later in Settings.
    It must not use the words `flock`, `mmap`, `Tantivy` or `FUSE`.
44. The user's choice must be recorded in the log, including the `is_removable`
    verdict, so a later "search is slow" report can be matched to it without
    guesswork.
45. The notice must be shown once per selection action, not repeated on every
    launch.

**Note on wording:** phase 1's diagnostics (FR-21 of the diagnostics PRD) report
per-probe elapsed times from real devices. If those numbers arrive before this is
implemented, prefer concrete wording over vague wording — but do **not** state a
specific slowdown factor we have not measured.

## 5. Non-Goals (Out of Scope)

- **Moving the index to internal storage.** The index is ~597 MB
  (`dict_words` 335 MB, `suttas` 237 MB, `library` 25 MB); forcing it onto
  internal storage defeats the purpose of choosing an SD card and would require
  affected users to move or re-download it.
- **Blocking SD cards at selection time.** With the fix in place they are usable.
- **Making the fallback lock safe across multiple processes.** Simsapa is a single
  process; a cross-process lock on a filesystem with no working `flock` is not
  solvable in-app.
- **Upgrading or forking Tantivy.** The wrapper is a `Directory` implementation
  against the public trait; no vendoring.
- **Changing how `StorageDialog` ranks, orders or classifies candidate volumes.**
  §4.7 adds a confirmation step to the *selection* flow; the scan, the ordering
  and the usable/unusable classification are untouched.
- **Blocking, discouraging or de-ranking removable volumes.** §4.7 sets
  expectations; it does not steer the choice.
- **The `scan_source failed: Path not found:` errors in the same log.** These are
  a separate defect in opening dictionary `.zip` files and will be handled in
  their own PRD.
- **Any change to ContainsMatch / FTS5 / SQLite behaviour**, which is unaffected.

## 6. Design Considerations

- **Search results area** — the message must reuse the existing empty-state
  rendering path in the results view rather than introduce a new dialog or
  toast. A search that fails should not interrupt the user with a modal.
- **Database Validation dialog** — the new row must look and behave exactly like
  the existing per-database rows (name, valid/invalid indicator, message), so no
  new visual language is introduced.
- **Wording** must avoid the words `flock`, `ENOSYS`, `Tantivy` and `META_LOCK`
  in user-facing text. The user-facing concept is *"this storage location does not
  support the file locking the search index needs; Simsapa is working around
  it"* — and, in the failure case, *"the search index could not be opened."*
- **QML logging** in any touched QML must use the `Logger` component
  (`logger.info` / `logger.error` with a single concatenated string), never the
  `console` API — see CLAUDE.md.

## 7. Technical Considerations

### Where the lock comes from (verified against tantivy 0.25.0)

| Site | Lock | Blocking? | Failure mapping |
|---|---|---|---|
| `reader/mod.rs:194` — `IndexReader` build/reload | `META_LOCK` | **yes** | `LockError::IoError(ENOSYS)` — this is the one in the log |
| `index/index.rs:545` — `Index::writer()` | `INDEX_WRITER_LOCK` | no | **`LockError::LockBusy`** — `try_lock_exclusive().map_err(\|_\| LockBusy)` swallows the real errno |

`MmapDirectory::acquire_lock` is at `mmap_directory.rs:476`. The lock file names
are `.tantivy-meta.lock` and `.tantivy-writer.lock`
(`directory/directory_lock.rs:45,57`).

These are the **only two** lock sites in the crate. In particular
`Index::open_or_create` (`index/index.rs:218`) and `Index::open` (`:510`) take
**no lock** — they are `ManagedDirectory::wrap` + `load_metas`. The log confirms
this: `list_indexed_source_uids` logs distinct messages for the open step
(`indexer.rs:824`) and the reader step (`indexer.rs:833`), and only the reader
message ever appears.

The `INDEX_WRITER_LOCK` errno-swallowing is why FR-5 needs the FR-7/FR-8 probe:
on the write path the error type alone cannot tell "unsupported" from
"contended".

### Why the wrapper is transparent

`MmapDirectory`'s `impl Directory` defines exactly nine methods —
`get_file_handle`, `delete`, `exists`, `open_write`, `atomic_read`,
`atomic_write`, `acquire_lock`, `watch`, `sync_directory`. It does **not**
override `open_read`, whose default calls `get_file_handle`, so delegating those
nine covers the whole trait. `DirectoryClone` has a blanket impl for
`T: Directory + Clone` and `MmapDirectory` is `Clone`, so the wrapper needs only
`#[derive(Clone)]`.

Nothing in tantivy ever downcasts a `Box<dyn Directory>` back to a concrete
`MmapDirectory` (verified by grepping the crate for `downcast` / `is::<` /
`as_any` — the only downcasting is on `Fruit` and `Scorer`). There is no hidden
fast path lost by wrapping. All nine methods are *required* (no defaults), so a
future tantivy that adds one breaks the build loudly rather than mis-delegating
silently.

See `2026-08-05-193727-research---tantivy-directory-wrapper-implications.md` for
the full analysis behind FR-11 through FR-18 and FR-34..36.

### Affected files (expected)

- `backend/src/search/` — new module for the lenient directory (e.g.
  `lenient_directory.rs`), plus edits to `searcher.rs` and `indexer.rs`.
- `backend/src/lib.rs` — `is_fulltext_searcher_ready()`,
  `reinit_fulltext_searcher()`, and the new index-failure report global.
- `backend/src/storage_probe.rs` — the `flock` probe (diagnostics only).
- `bridges/src/sutta_bridge.rs` — the "Fulltext index" validation result.
- `assets/qml/DatabaseValidationDialog.qml` — the new row.
- The search results QML — the "index could not be opened" empty state.
- `bridges/src/api.rs` — `/health`'s `fulltext_searcher_ready` becomes honest as a
  consequence of FR-13; consider also reporting the per-area index counts.
- `assets/qml/StorageDialog.qml` and `assets/qml/StorageRecoveryWindow.qml` — the
  §4.7 removable-volume notice. The hook points are the **Select** button's
  `onClicked` (`StorageDialog.qml:254`, which currently calls
  `save_selected_path()` then `root.accept()`) and `auto_select_single_location()`
  (`:95`). The `is_removable` field is already carried through the tier-1 scan
  into the list rows, so no new backend data is needed.

### Compatibility notes

- No database migration, no DB version bump, no index format change. Existing
  index files on SD cards are valid and are read as-is once the lock stops
  failing — this is what satisfies Goal 3.
- If the QML side gains new components or new bridge functions, remember the
  project rules in CLAUDE.md: new `.qml` files go in `bridges/build.rs`'s
  `qml_files` list, and new bridge methods need a matching stub in
  `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` for `qmllint`.
- `try_exists()` — not `.exists()` — for any new file-existence check, per the
  Android rule in CLAUDE.md.

### Documentation

- Add a new doc, `docs/fulltext-index-storage-and-file-locking.md`, covering the
  `flock`-vs-`fcntl` distinction, why SQLite and the storage probe pass while
  Tantivy fails, the two Tantivy lock sites and their differing failure mappings,
  the single-process invariant the fallback depends on, and the diagnostic path.
  Link it from `CLAUDE.md`'s notable-feature-docs list.
- Cross-link it from `docs/relocated-storage-recovery.md` (which owns the storage
  probe) and `docs/search-snippet-highlight-pipeline.md`.

## 8. Success Metrics

1. On an Android device with the storage location set to an SD card, the log
   reports **non-zero** index counts:
   `FulltextSearcher opened: N sutta …, M dict …, K library …` with N, M, K > 0.
2. A FulltextMatch search for a known term on that device returns the **same
   result count** as the identical search with storage on internal storage.
3. Library search returns results on that device.
4. `Rebuild search index`, a StarDict dictionary import, and a Library book import
   all complete successfully on that device.
5. An affected user who updates the app and restarts gets working search **without
   re-downloading any assets**.
6. The two reporting users (the SD-card tester and the ChromeOS user) confirm
   fulltext results appear.
7. No change in index-open time or search latency on desktop Linux (the `flock`
   fast path is unchanged; the probe runs at most once per index directory).
8. `cd backend && cargo test` passes, including a new unit test that exercises the
   lenient directory's fallback path with a simulated unsupported-lock inner
   directory.
9. No `thread-tantivy-meta-file-watcher` threads exist in the running app (FR-17),
   and index changes are still picked up after a rebuild/import — confirming the
   explicit `reinit_fulltext_searcher()` fully replaces the 500 ms poll.
10. The startup log carries both probe verdicts (`flock`, `mmap`) for the resolved
    storage path on every platform.
11. Selecting a removable volume shows the performance notice; selecting internal
    storage does not. Cancelling the notice records nothing and returns to the
    list with the selection intact.
12. No user reports "searches are slow on my SD card" as a suspected bug after the
    notice ships — or, if they do, the log's recorded `is_removable` verdict
    (FR-44) settles it immediately.

## 9. Open Questions

1. ~~**Does `mmap` behave correctly on this FUSE/exFAT mount?**~~ — **Resolved
   into requirements FR-34..36** (§4.6): rather than leaving this as an open
   question to be answered on hardware we do not have, the probe now measures it
   and logs the verdict, and must ship first. The question itself remains open
   until that data arrives.
2. **Which volumes exactly are affected?** Confirmed: Android portable SD card via
   FUSE (`/storage/<UUID>/…`). Unknown: adopted-storage SD cards, USB-OTG mounts,
   and the ChromeOS/ARCVM path specifically — worth collecting the probe verdict
   in logs (FR-26) to find out.
3. **Should the `flock` verdict itself be shown in `StorageDialog`?** Partly
   answered: §4.7 now shows a **performance** notice for removable volumes,
   gated on `is_removable` — not on the `flock` verdict, which stays a
   diagnostics-only record (FR-32). Remaining question: if phase 1 shows the
   workaround carries a *measurable* cost beyond ordinary card slowness, should
   the notice say so? Answerable only from phase 1 timings.
4. **Should the fulltext-index row also appear in the startup `StartupDbReport`**
   so a failure is visible before the user opens Database Validation?
5. **Retention of the ChromeOS `scan_source` defect** — filed separately as noted
   in Non-Goals; confirm the separate PRD exists before closing this one.
