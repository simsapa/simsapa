# Tasks — "Run Storage Diagnostics" (phase 1)

PRD: `tasks/2026-08-05-201545-prd---run-storage-diagnostics.md`
Phase 2 (the fix): `tasks/2026-08-05-193727-prd---fulltext-index-on-filesystems-without-flock.md`
Research: `tasks/2026-08-05-193727-research---tantivy-directory-wrapper-implications.md`

## Component analysis (what has to exist, and what blocks what)

| # | Component | Kind | Depends on | PRD FRs |
|---|---|---|---|---|
| C1 | `flock` support probe (shared by wrapper + section B) | Rust | — | 17, 39, 43 |
| C2 | `LenientLockMmapDirectory` in its **final** location | Rust | C1 | 27, 30, 46 |
| C3 | Mount-table / filesystem-type parser | Rust (pure, testable) | — | 15 |
| C4 | Storage-location facts (recorded vs resolved path, state, space) | Rust | C3 | 12–16 |
| C5 | Primitive probes: mmap, atomic-write, plain read/write | Rust | — | 18–21, 38–39 |
| C6 | Index inventory walker | Rust | — | 22–24, 38a |
| C7 | Tantivy open — current sequence, per-step attribution | Rust | — | 25–26, 26a/b, 40 |
| C8 | Tantivy open — through the wrapper + query + `num_docs` | Rust | C2, C7 | 27–30, 27a, 28a–d |
| C9 | Live-searcher state + platform/version facts | Rust | — | 31–32, 31a/b |
| C10 | Verdict deriver (plain language, from measured results) | Rust (pure, testable) | C4–C9 result types | 33–37 |
| C11 | Report builder → `String`, INFO-logged | Rust | C4–C10 | 11, 45 |
| C12 | `SuttaBridge` async invokable + completion signal + qmllint stub | Bridge | C11 | 4, 47 |
| C13 | `StorageDiagnosticsDialog.qml` (monospace, selectable, Copy/Close; owns the run) | QML | C12 | 6–10, 10a–c, 48 |
| C14 | Entry-point buttons in About + Database Validation dialogs | QML | C13 | 1–3, 5 |
| C15 | Unit tests (report builder, mount parser, verdict) | Rust tests | C3, C10, C11 | metrics 7 |
| C16 | Documentation update | docs | C11 | §7 |

There is deliberately **no CLI or `/health` component**: the UI button is the only
caller (PRD Non-Goals, §11.2).

Every PRD functional requirement FR-1..FR-48 maps to at least one component above.

## Notes on the existing codebase (assessed before planning)

- `backend/src/storage_probe.rs` is the model for the `Drop`-guard cleanup and
  the distinctive probe filename (FR-39, §7).
- `backend/src/lib.rs` already has `storage_path_state()` /
  `storage_path_state_of_file()` returning `(StorageState, Option<PathBuf>)`
  (FR-12, FR-13), `AppGlobalPaths` with `suttas_index_dir`,
  `dict_words_index_dir`, `library_index_dir` (FR-22), the process-global
  `FULLTEXT_SEARCHER` (`lib.rs:169`), `with_fulltext_searcher()` (`lib.rs:369`)
  and `is_fulltext_searcher_ready()` (`lib.rs:359`).
- `backend/src/search/searcher.rs:127` (`open_single_index`) is the **current**
  open sequence to reproduce in section D — note it uses
  `Index::open_or_create`, which the diagnostic must **not** use (FR-40). The
  per-directory `warn()` that FR-31 hooks into is at `:120`.
- `backend/src/search/indexer.rs:609-645` already exposes `INDEX_VERSION`,
  `write_version_file()`, `read_version_file()` and `is_index_current()` for the
  **top-level** `index/VERSION` file (FR-24) — reuse them; no change needed
  there.
- `bridges/src/storage_manager.rs:138-168` (`probe_storage_candidate`) is the
  working pattern for `thread::spawn` + `qt_thread.queue(...)` + a signal, plus
  generation-counter cancellation. `sutta_bridge.rs:3879` (`rebuild_search_index`)
  is the same pattern on `SuttaBridge`, with its
  `rebuildSearchIndexCompleted(bool, QString)` signal at `sutta_bridge.rs:819`.
- `assets/qml/AboutDialog.qml:63-71` holds the invisible-`TextEdit`
  `clipboard_helper` to reuse (FR-7); its bottom button row is the insertion
  point (FR-1).
- `docs/relocated-storage-recovery.md` (recorded vs resolved path) and
  `docs/android-edge-to-edge-and-safe-areas.md` (dialog sizing) govern FR-12 and
  FR-10.
- `tantivy = "0.25"` and `tempfile = "3"` are already in `backend/Cargo.toml`.
  `fs4 0.13.1` and `memmap2 0.9.10` are already in `backend/Cargo.lock`
  transitively via tantivy; adding `fs4 = "0.13"` and `memmap2 = "0.9"` as direct
  dependencies resolves to those same versions, so there is exactly one copy.
  fs4's default features include `sync`, which is what provides
  `fs4::fs_std::FileExt` — no feature flags needed.
- `libc` is **not** a dependency of `backend`, and `sysinfo` is deliberately
  avoided (it needs a higher Android API level — see the note at
  `backend/src/app_data.rs:5017`). The established pattern for platform facts is
  per-`cfg` functions, as in `get_system_memory_bytes()` /
  `get_cpu_cores()` (`app_data.rs:5019-5070`).

## Review findings (phase 3) — verified against the source, not assumed

These were found by reading `tantivy-0.25.0` and the app's own code after the
task list was drafted. Each one is folded into the sub-tasks below; they are
collected here so the reasoning is not lost.

1. **`Index::open` takes no schema.** `Index::open<T: Into<Box<dyn Directory>>>(directory)`
   (`index/index.rs:510`) reads the schema from `meta.json`. Only
   `register_tokenizers(&index, lang)` is still needed, *after* the open and
   before querying, because the stored schema names `{lang}_stem` /
   `{lang}_normalize` tokenizers that must be registered on that `Index`
   instance. (An earlier draft of task 3.5 wrongly passed a schema.)
2. **The trait's default `acquire_lock` is not a usable fallback**, and this must
   be recorded at the fallback so nobody "simplifies" the wrapper into it. The
   default (`directory/directory.rs:190-208`) locks by *file existence*:
   `open_write` → `FileAlreadyExists` → `LockBusy`, with the guard **deleting**
   the file on drop. But `MmapDirectory` never deletes its lock files —
   `ReleaseLockFile`'s `Drop` only closes the fd, which is precisely why FR-23
   expects stale `.tantivy-meta.lock` files to be lying about. A file-existence
   fallback would therefore find the leftover file and return `LockBusy`
   **forever**. The process-internal mutex (fix-PRD FR-4..FR-6) is the correct
   design.
3. **`ReloadPolicy::Manual` does not avoid the failing lock.**
   `IndexReader::open_segment_readers()` takes `META_LOCK` unconditionally
   (`reader/mod.rs:194`) on every reader build, whatever the reload policy. So a
   section E success is attributable to the **wrapper**, not to `Manual`, and the
   report must not let a reader conclude otherwise.
4. **The wrapper must be `Clone + Debug`.** `Directory: DirectoryClone + fmt::Debug
   + Send + Sync + 'static`, and `DirectoryClone` is blanket-implemented only for
   `T: Directory + Clone` (`directory.rs:246-252`). This dictates the struct
   shape (a clonable/`Arc`-held inner).
5. **Section D diverges from the app on a missing `meta.json`.** The app calls
   `Index::open_or_create` (`searcher.rs:137`), which *creates* an empty index and
   then returns zero results; FR-40/FR-25 make the diagnostic call `Index::open`,
   which **fails** there instead. That divergence is wanted — it is how "index
   missing or incomplete" gets detected (FR-34) — but section D must say so in its
   wording, or the failure reads as one the app does not actually have.
6. **Section F can be legitimately empty.** `init_fulltext_searcher()` is lazy and
   mode-gated (called from `run_search`), so if no Fulltext query has run this
   session, `FULLTEXT_SEARCHER` is `None`. FR-41 forbids initialising it. Section F
   must print **"searcher not initialised this session"** as a state distinct from
   "0 indexes", or the verdict mis-fires on a perfectly healthy install.
7. **FR-14 and FR-16 have no Rust data source today.** Free/total space and
   internal-vs-external reach `scan_storage_candidates(enumeration_json, …)`
   (`lib.rs:829`) already computed, from the C++/JNI enumeration. New per-`cfg`
   helpers are needed — see task 2.4.
8. **`get_app_globals()` panics when uninitialised** (`lib.rs:180`,
   `.expect("AppGlobals is not initialized")`). This is safe here: `gui.cpp:403`
   calls `init_app_globals()` unconditionally and before any dialog exists, and
   the GUI button is now the **only** caller (no CLI, no `/health` — PRD
   Non-Goals). Keep the assumption documented at the entry point anyway, so a
   future headless caller does not inherit it silently.
9. **The library index is normally empty** (most users import no books), so zero
   hits there is expected and must not feed a fault verdict.
10. **Was undecided in the PRD, now settled:** FR-28 named no English term.
    Decided 2026-08-06 — **`nirodha`** for Pāli indexes, **`cessation`** for
    non-Pāli ones — and written into FR-28 and PRD §10.3.
11. **`fs4` already provides cross-platform free/total space — no `libc`, no
    Windows branch.** `fs4 0.13.1` exports free functions `statvfs(path)`,
    `free_space`, `available_space`, `total_space` (`fs4-0.13.1/src/lib.rs:199-226`),
    implemented over rustix on unix/Android (`src/unix.rs:75`) and over
    `GetDiskFreeSpaceExW` on Windows (`src/windows.rs:118`). Since task 1.1 adds
    `fs4` as a direct dependency anyway, FR-14 costs three lines. An earlier draft
    of task 2.4 planned per-`cfg` `statvfs` / `GetDiskFreeSpaceExW` helpers plus a
    new `libc` dependency; that is unnecessary.
12. **…but `fs4` is deliberately libc-free**, so it supplies no errno *constants*.
    FR-17's `Unsupported { errno, name }` still needs `libc` (for `ENOSYS`,
    `EINVAL`, `EOPNOTSUPP`) or a small hardcoded per-`cfg` table. Note also that
    **`EOPNOTSUPP == ENOTSUP == 95` on Linux/Android** — FR-17 names four
    classifications but only three distinct numbers exist there, so the
    errno→name lookup must not pretend to distinguish them.
13. **`storage_path_state()` returns `(Absent, None)` on every desktop.**
    `lib.rs:1080-1082` short-circuits on `!is_mobile()`. So on a healthy
    desktop install section A reports state `absent` with no recorded path —
    which is exactly the input FR-34's *"the storage location is unreachable"*
    branch keys on, and printing it bare would violate FR-37. The recorded-path
    notion is mobile-only and section A must say so; `derive_verdict` must gate
    that branch on mobile.
14. **All three schemas carry the same query fields.** `build_sutta_schema`
    (`schema.rs:48-49`), `build_dict_schema` (`:152-153`) and
    `build_library_schema` each define `content` (`{lang}_stem`) and
    `content_exact` (`{lang}_normalize`). The live search builds a dual-field
    Must/Should boolean with a boost (`searcher.rs:515-525`); the diagnostic does
    **not** need to reproduce that — a single `QueryParser::for_index(index,
    vec![content])` is sufficient, uniform across all three areas, and less to get
    wrong. `register_tokenizers` (`tokenizer.rs:186`) is already `pub`.
15. **Section D leaks a watcher thread per index if the readers are not dropped.**
    Section D deliberately reproduces the app's bare `index.reader()`, i.e. the
    default `ReloadPolicy::OnCommitWithDelay`, which spawns a 500 ms polling
    thread per index for the reader's lifetime (research §8). Six leaked threads
    per diagnostic run contradicts Goal 4 and success metric 6, so the section-D
    readers must be dropped explicitly at the end of each directory's steps.
16. **The two entry-point dialogs are `ApplicationWindow` roots, not `Dialog`s**,
    instantiated as inline siblings of `SuttaSearchWindow.qml` — `AboutDialog` at
    `:2257`, `DatabaseValidationDialog` at `:2277`. The shared results dialog is
    therefore a **third sibling declared in `SuttaSearchWindow.qml`**, a file the
    earlier draft never mentioned. Two consequences: per
    `docs/startup-sequence-and-caches.md` an `ApplicationWindow` root cannot be
    wrapped in a `Loader` (it needs `Component` + `createObject` to defer), and an
    eager sibling's `Component.onCompleted` runs inside the engine load, before
    `app.exec()`.
17. **`canonicalize()` can fail on an Android FUSE path.** Canonicalising is
    correct here — fix-PRD FR-6 requires it so two `Directory` instances on one
    index directory share one mutex — but `docs/relocated-storage-recovery.md`
    records that the storage code deliberately never canonicalises, because it can
    fail on exactly the volumes this PRD targets. The fallback must be "use the
    absolute path as-is", never "skip the entry": skipping hands the two instances
    two different mutexes, which is the precise bug FR-6 exists to prevent.
18. **`acquire_lock` has a third failure source that also surfaces as `IoError`.**
    `MmapDirectory::acquire_lock` opens the lock file before locking it, so a
    read-only or otherwise unwritable directory fails at `open_write` and produces
    `LockError::IoError` too — which the FR-4 fallback would silently convert into
    a *successful* process-internal lock. For phase 2 that is a masked failure; for
    the diagnostic it is the difference between "the fix works" and "the fix hid
    the problem". Section E's lock-path line must therefore distinguish "fell back
    after an unsupported-operation errno" from "fell back after some other
    `IoError`".
19. **The FR-31 failure global needs a clear-on-reopen rule.** It is populated
    where `searcher.rs:121` warns, i.e. only when a searcher is actually built. It
    must be cleared at the top of `FulltextSearcher::open` /
    `reinit_fulltext_searcher()`, or an entry recorded before a storage recovery
    survives and section F reports a failure that no longer exists. And because it
    shares the searcher's lifecycle, "searcher not initialised this session"
    (finding 6) applies to the failure list as well as to the counts.
20. **~~`rebuildSearchIndexCompleted` has no `qmllint` stub today.~~ — WRONG,
    corrected by the phase-5 review; see finding 22.** The stub exists.
21. **The mmap probe must select its file by size, not by extension.** An
    extension allowlist will happily pick a 200-byte `.fast` file, which cannot
    fault past page 0 — defeating the entire purpose of the middle and last reads.
    The rule is "largest file at least ~8 KiB", with the diagnostic's own
    multi-page file as the fallback. (Confirmed on the shipped tree: the
    per-language directories hold `.store` / `.pos` / `.term` files of tens of MB
    alongside 146-byte `.fast` files, so the ~8 KiB floor selects correctly.)

## Review findings (phase 5) — one correction and eleven gaps

A second verification pass (2026-08-06) re-checked every source claim above
against the tree. Findings 1–19 and 21 all held. Finding 20 did not, and eleven
gaps surfaced that the earlier passes had not reached. Each is folded into the
sub-tasks below and into the PRD.

22. **Finding 20 was wrong: the `rebuildSearchIndexCompleted` stub exists.**
    `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` has a "Search index
    signals" group declaring `rebuildSearchIndexProgress` (`:45`) and
    `rebuildSearchIndexCompleted` (`:46`); the signal block runs to `:51`, and
    there are further signals at `:1157-1162`. So the neighbouring code **is** the
    model — add `storageDiagnosticsCompleted` to that same group. This matters
    beyond tidiness: a task that says "do not go looking for a precedent" produces
    a stub that does not match the file's own convention.
23. **The `VERSION` file is one top-level file, and a public reader already
    exists.** `write_version_file(&paths.index_dir)` (`indexer.rs:599`) writes
    `<app-assets>/index/VERSION` — one file for the whole tree, current contents
    `1.0` — **not** one per language directory, where FR-24/task 3.4 placed it
    inside the per-directory section. `read_version_file()`, `is_index_current()`
    and `INDEX_VERSION` are public at `indexer.rs:609-645` and already consumed by
    `sutta_bridge.rs:3856`. Report it once, at section-C header level, **with** the
    `INDEX_VERSION` comparison — a mismatch is a real "index stale or incomplete"
    input the FR-34 verdict otherwise cannot see.
24. **`DatabaseValidationDialog` is `Qt.ApplicationModal` (`:18`) and will
    input-block a non-modal results window.** `AboutDialog` sets no modality, so
    the About entry point — implemented and tested first — does not expose the
    bug; the Database Validation path shows the window and leaves it dead to
    clicks. A modal window shown *later* heads the modal stack and is not itself
    blocked, so the fix is to make the results window `Qt.ApplicationModal` too
    (PRD FR-10a).
25. **`extra_top_margin` is a `required property` on every dialog sibling.**
    `AboutDialog.qml:29` and `DatabaseValidationDialog.qml:25` declare it; every
    declaration in `SuttaSearchWindow.qml:2257-2295` binds
    `extra_top_margin: root.extra_top_margin`. An unbound `required property` is a
    **runtime** QML error — `make build -B` passes and the window fails to
    instantiate on first open (PRD FR-10b).
26. **`cessation` produces a false fault on `suttas/san`.** The shipped tree is
    `suttas/{en,pli,san}`, `dict_words/{en,pli}`, `library/en`. FR-28's original
    "Pāli → `nirodha`, everything else → `cessation`" sends an English term at an
    index of romanized Sanskrit: a **populated** index returns zero, which task
    4.4 classifies as the *unexpected* case, which feeds task 5.4's verdict. Every
    healthy device with Sanskrit installed reports a fault. Resolved by PRD
    FR-28d — run **both** terms against **every** index and report both counts.
27. **Nothing measures whether an index has documents, though three places depend
    on it.** Tasks 4.4, 5.4 and FR-28b/FR-37 all turn on "an index that *does*
    contain documents", but section C counts files and sections D/E measure opens.
    `reader.searcher().num_docs()` on the reader section E already holds answers it
    in one line, and is an independent read-path proof if both query terms are
    unlucky (PRD FR-28c).
28. **Section D creates `.tantivy-meta.lock` files that were not there before.**
    `MmapDirectory::acquire_lock` opens the lock file *before* locking it (the same
    fact finding 18 rests on), so `index.reader()` in section D writes into the
    user's index directory on a card that has never opened successfully — and the
    FR-39 `Drop` guards do not remove it, being scoped to `simsapa-…` names. This
    is in tension with FR-38 and Goal 5. Accepted rather than prevented (the files
    are zero-length, tantivy never deletes them anyway, any startup creates them),
    but it forces two things: section C's lock reading must be **collected before
    section D executes**, and a creation must be stated in one line (PRD FR-38a).
29. **The FR-31 clear misses the second constructor.** `FulltextSearcher` has two
    — `open()` (`searcher.rs:55`) and `open_from_dirs()` (`:78`) — and both call
    `open_indexes()` three times. Clearing only in `open()` leaves
    `open_from_dirs()` appending to a list that is never reset (PRD FR-31a).
30. **`is_fulltext_searcher_ready()` must not be the source for "not
    initialised", and must not be fixed here.** `lib.rs:358-360` returns `true`
    whenever the global is `Some`, regardless of index count. Derive the state from
    `with_fulltext_searcher()` (`lib.rs:369`) returning `None`. Correcting the
    function itself is phase-2 FR-20; doing it in phase 1 changes `/health`'s
    `fulltext_searcher_ready` and breaks Goal 4 / metric 6 (PRD FR-31b).
31. **The section-E `get_field("content")` lookup can legitimately fail.** With
    `Index::open` the schema comes from `meta.json` rather than being passed in, so
    a foreign, truncated or older `meta.json` yields no such field and
    `schema().get_field()` returns `Err`. That is an attributed output line, not a
    `?` that aborts the section (FR-44).
32. **Section D should not call `register_tokenizers`.** It measures opens and
    never queries; registration is load-bearing only in section E, where the
    `QueryParser` resolves the tokenizers off the `Index` at parse time. Task 3.5
    carried it over from an earlier draft and implies a parity with E that does not
    exist (PRD FR-26b).
33. ~~**The CLI-init instruction was imprecise.**~~ — **moot.** Decided
    2026-08-06: there is **no CLI subcommand and no `/health` exposure** (PRD
    Non-Goals, §11.2). The UI button is the only caller, so task 8.3 is deleted
    and the `init_app_globals()` question does not arise — `gui.cpp:403` has
    already initialised the globals before any dialog can exist.

**Line-number corrections** to findings above and to the notes section:
`open_single_index` is `searcher.rs:127` (not `:136`); the `warn()` that FR-31
hooks into is `searcher.rs:120` (not `:121`); the sutta `content` /
`content_exact` pair is `schema.rs:47-48` (not `:48-49`), the library pair is
`:99-100`, and the dictionary pair at `:152-153` is correct as stated.

## Review findings (phase 6) — five defects in the implemented sections, all fixed

A review of the implemented tasks 1.0–3.0 against the source (2026-08-06, after
commit `b1bb617`). Every PRD-critical property held — no `open_or_create`, no
`FULLTEXT_SEARCHER` access, readers dropped, the lock snapshot taken pre-run,
`VERSION` read once at tree level, the desktop-state flag wired through. Five
defects were found and fixed in place; two questions went to PRD §11 (3 and 4).

34. **`last_lock_path` kept only the most recent route** and FR-27a depends on
    it. One `index.reader()` reaches `acquire_lock` more than once
    (`open_segment_readers()` takes `META_LOCK` on every reader build,
    `reader/mod.rs:194`), so an early `FallbackOtherIoError` was overwritten by a
    later `InnerFlock` — the "the fix works" versus "the fix hid the failure"
    pair the three-way split exists for. Now a `Vec` of distinct routes behind
    `lock_paths()`, with `last_lock_path()` kept as a convenience. Task 4.2 must
    use the former.
35. **The errno classification was wrong on Windows.** `errno_name()` /
    `errno_is_unsupported()` compared `io::Error::raw_os_error()` against libc's
    **CRT** errno constants, but on Windows that call returns **Win32** codes
    from `LockFileEx`, and the numbering overlaps meaninglessly (libc's Windows
    `EOPNOTSUPP` is 130 = `ERROR_DIRECT_ACCESS_HANDLE`; `EINVAL` is 22 =
    `ERROR_BAD_COMMAND`). An ordinary Windows error could be classified
    `Unsupported`, which is cached for the process lifetime and permanently skips
    the inner lock for that directory (task 1.6a). Now `#[cfg(unix)]` for the
    errno table, a Windows arm naming only `ERROR_INVALID_FUNCTION` (1) and
    `ERROR_NOT_SUPPORTED` (50), and a never-`Unsupported` fallback elsewhere;
    `libc` moved to `[target.'cfg(unix)'.dependencies]`.
36. **The `LockBusy` → unsupported branch was unreachable.** `acquire_lock`
    early-returns when the cached support is `Unsupported`, so
    `support.is_unsupported()` in that arm was always false. Replaced with a
    plain propagate plus the reasoning, so nobody re-adds a mask that would let
    two writers into one index.
37. **`enumerate_index_dirs_in` used `path.is_dir()`**, which reports a
    permission error as `false` — silently dropping an index directory from
    sections C, D and E, an invisible hole in the report on exactly the volumes
    under investigation. Now `entry.file_type()`, with the error logged.
38. **`probe_mmap` would have been UB on a zero-length file** (`read_volatile` at
    offset 0 of an empty map). Unreachable given the ~8 KiB floor and the
    fallback file, but now guarded explicitly.

## Relevant Files

- `backend/src/search/lenient_directory.rs` — **new.** `LenientLockMmapDirectory`
  and the per-directory `flock` support probe + cache, in the location phase 2
  will use unchanged (FR-30, FR-46).
- `backend/src/search/mod.rs` — declare the new `lenient_directory` module.
- `backend/src/storage_diagnostics.rs` — **new.** All probes, the section
  builders, the verdict deriver and the report builder returning a `String`
  (FR-45).
- Mount-table parser — implemented as `parse_mount_table()` **inside**
  `storage_diagnostics.rs` (the "module inside the file above" option), pure and
  fixture-tested (FR-15). No separate file was needed.
- `backend/src/lib.rs` — declare the new modules; add a diagnostics-only record
  of per-index-directory open failures (FR-31) and expose the searcher's index
  counts.
- `backend/src/search/searcher.rs` — record open failures into that global in
  `open_indexes()` (currently only `warn()`, `:120`); clear it from a helper
  called by **both** constructors, `open()` (`:55`) and `open_from_dirs()`
  (`:78`) — finding 29; add a public index-count accessor (the
  `has_*_indexes()` predicates at `:270`, `:275`, `:313` answer a different
  question). **No behaviour change**, and in particular
  `is_fulltext_searcher_ready()` is **not** touched (finding 30).
- `backend/src/search/indexer.rs` — **read only.** Source of `INDEX_VERSION` /
  `read_version_file()` / `is_index_current()` for FR-24 (finding 23).
- `backend/src/storage_probe.rs` — reference pattern for the `Drop` cleanup
  guard; no change expected.
- `backend/src/app_data.rs:5019-5070` — the per-`cfg` platform-facts pattern
  (`get_system_memory_bytes()`, `get_cpu_cores()`); `sysinfo` is rejected there
  for Android. Free/total space does **not** need this pattern — `fs4` covers it
  cross-platform (review finding 11) — but the platform/API-level facts of FR-32
  do.
- `backend/Cargo.toml` — add `fs4 = "0.13"` and `memmap2 = "0.9"`, plus `libc`
  **only** for the errno constants of FR-17 (review finding 12). All three are
  already in `Cargo.lock` transitively at 0.13.1 / 0.9.10 / 0.2.186.
- `bridges/src/sutta_bridge.rs` — the `run_storage_diagnostics()` invokable and
  the `storageDiagnosticsCompleted` signal.
- `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` — `qmllint` stub for the
  new method and signal (FR-47).
- `assets/qml/StorageDiagnosticsDialog.qml` — **new.** The results dialog
  (FR-6..FR-11). `ApplicationWindow` root, `modality: Qt.ApplicationModal`
  (finding 24), `required property int extra_top_margin` (finding 25), and the
  **sole owner** of the run: `open_and_run()`, the `Connections`, the
  "initiated here" guard, the busy state and its own `AssetManager` (FR-10c).
- `assets/qml/AboutDialog.qml` — the "Run Storage Diagnostics" button (FR-1),
  which only calls `open_and_run()`. Root is an **`ApplicationWindow`** with no
  modality set. It needs **no** `AssetManager`: the keep-screen-on bracket lives
  in the results dialog (FR-10c).
- `assets/qml/DatabaseValidationDialog.qml` — the second entry point (FR-2).
  Also an `ApplicationWindow` root, and `modality: Qt.ApplicationModal` (`:18`) —
  the reason for finding 24. Its `AssetManager { id: manager }` (`:297`) with the
  keep-screen-on bracket at `:305`/`:326` is the **shape to copy into the results
  dialog**, not a hook to reuse from here.
- `assets/qml/SuttaSearchWindow.qml` — where the shared results dialog is
  **declared**, as a third sibling alongside `AboutDialog` (`:2257`) and
  `DatabaseValidationDialog` (`:2277`). Both entry points open the one instance
  rather than each carrying its own (review finding 16).
- `bridges/build.rs` — add `StorageDiagnosticsDialog.qml` to `qml_files`.
- `backend/src/storage_diagnostics.rs` (test module) — unit tests for the mount
  parser, the verdict deriver and the report builder, using fixture inputs.
- `cli/src/main.rs`, `bridges/src/api.rs` — **not touched.** No CLI subcommand and
  no `/health` exposure: the UI button is the only caller (PRD Non-Goals, §11.2).
- `docs/storage-diagnostics.md` — **new.** What the report contains, how to read
  it, and the decision gate table.
- `CLAUDE.md` / `PROJECT_MAP.md` — pointers to the new doc and modules.

### Notes

- Rust tests: `cd backend && cargo test`; a single test with
  `cargo test test_name`. QML: `make qml-test`. Full build: `make build -B`.
- Do **not** run the GUI to test (see CLAUDE.md); compile-verify and unit-test.
- Every file-existence check uses `try_exists()`, never `.exists()` (FR-43).
- QML logging uses `Logger { id: logger }` with a **single concatenated string**
  (FR-48).

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, check it off in this markdown file by
changing `- [ ]` to `- [x]`. Update the file after completing each sub-task, not
just after completing an entire parent task.

## Tasks

---

### 1.0 Backend foundation: the `flock` probe and the `LenientLockMmapDirectory` wrapper, in their final phase-2 locations

**Specs to keep in mind.** This is phase-2 code (fix PRD FR-1..FR-16, FR-8)
written now and wired **only** into the diagnostic. `MmapDirectory::acquire_lock`
uses `fs4::fs_std::FileExt` (`mmap_directory.rs:476`); the blocking `META_LOCK`
surfaces a real errno as `LockError::IoError`, while the non-blocking
`INDEX_WRITER_LOCK` path maps **everything** to `LockBusy` and discards the errno
(`index/index.rs:545`). `DirectoryLock` is `Box<dyn Send + Sync + 'static>`, so
the fallback guard must be hand-rolled — a `MutexGuard` is `!Send`.

**Depends on:** nothing. **Blocks:** 2.0 (shares the probe), 4.0 (needs the wrapper).

- [x] 1.1 Create `backend/src/search/lenient_directory.rs` and declare it in
      `backend/src/search/mod.rs`. Add `fs4 = "0.13"`, `memmap2 = "0.9"` and
      `libc` to `backend/Cargo.toml` — all three already in `Cargo.lock`
      transitively at 0.13.1 / 0.9.10 / 0.2.186, so this adds no second copy —
      and note in a comment that fs4's default `sync` feature is what provides
      `fs4::fs_std::FileExt`. `libc` is pulled in **only** for the errno
      constants of task 1.2: `fs4 0.13` is deliberately libc-free (rustix-based)
      and exports none (review finding 12).
- [x] 1.2 Implement `FlockSupport` — an enum with `Supported`, `Busy`,
      `Unsupported { errno: i32, name: String }`, `Error { errno: i32, message: String }`
      — and `probe_flock_support(dir: &Path) -> (FlockSupport, Duration)`. It
      creates/opens a distinctively-named probe file (e.g.
      `simsapa-flock-probe.tmp`), calls `try_lock_exclusive()`
      (`fs4` returns `std::io::Result<bool>`; read the errno with
      `io::Error::raw_os_error()`), classifies `ENOSYS` / `EOPNOTSUPP` /
      `ENOTSUP` / `EINVAL` as `Unsupported`, treats `Ok(false)` as `Busy`
      (**not** unsupported), releases the lock, and records the raw errno in
      every case (FR-17). The errno→name lookup must **not** claim to
      distinguish `EOPNOTSUPP` from `ENOTSUP`: they are the same number (95) on
      Linux and Android (review finding 12). Name the probe file distinctly
      enough that it cannot be confused with a tantivy segment file; tantivy's
      GC only ever deletes files it manages, so an unmanaged foreign file in an
      index directory is left alone — the `Drop` guard of 1.3 is what removes
      it, not tantivy.
- [x] 1.3 Give the probe a `Drop`-guard cleanup struct modelled on
      `ProbeCleanup` in `backend/src/storage_probe.rs`, removing the probe file on
      success, failure **and** panic (FR-39). Use `try_exists()` (FR-43).
- [x] 1.4 Add a process-global cache `HashMap<PathBuf, FlockSupport>` keyed on the
      canonicalised index-directory path, so support is determined **once per
      index directory** for the process lifetime (fix-PRD FR-7). Expose
      `flock_support_for_dir(&Path) -> FlockSupport` which consults the cache.
- [x] 1.5 Implement the process-internal fallback lock: a global table keyed on
      the **canonicalised absolute lock-file path** (fix-PRD FR-6), and a
      hand-rolled `Send + Sync` guard type over `Arc<Mutex<…>>`/`Condvar` that
      releases in `Drop` (fix-PRD FR-12). It must be real mutual exclusion, never
      a no-op (fix-PRD FR-13), and must preserve blocking semantics: blocking for
      `META_LOCK`, `try_lock` returning `LockBusy` for `INDEX_WRITER_LOCK`
      (fix-PRD FR-14).
- [x] 1.5a Write **one** shared key-normalisation helper used by both 1.4 and
      1.5, with an explicit `canonicalize()` fallback: on failure, use the
      absolutised path as-is (`std::path::absolute` / join against the cwd) —
      never skip or drop the entry. `canonicalize()` can fail on exactly the
      Android FUSE volumes this PRD targets, and
      `docs/relocated-storage-recovery.md` records that the storage code avoids
      it for that reason. Dropping the entry would hand two `Directory`
      instances on one index directory two different mutexes, which is the bug
      fix-PRD FR-6 exists to prevent (review finding 17). Unit-test that a
      path that cannot be canonicalised still maps two instances to one key.
- [x] 1.5b Add the **bounded retry loop** for the blocking (`META_LOCK`)
      fallback, mirroring tantivy's own `RetryPolicy { num_retries: 100,
      wait_in_ms: 100 }` (`directory/directory.rs:89`), logging an error on
      exhaustion rather than blocking unboundedly (fix-PRD FR-15). Tantivy does
      not nest `META_LOCK` acquisitions today, but that is an upstream detail;
      a bounded wait fails loudly instead of freezing a thread if a future
      version does.
- [x] 1.6 Implement `LenientLockMmapDirectory`, deriving/implementing **`Clone` and
      `Debug`** — `Directory: DirectoryClone + Debug + Send + Sync + 'static` and
      `DirectoryClone` is blanket-implemented only for `T: Directory + Clone`
      (`directory.rs:246-252`), so a non-`Clone` wrapper will not compile as a
      `Directory`. Wrap `MmapDirectory` (itself `Clone`) and delegate
      **every** `Directory` trait method to the inner directory unchanged, except
      `acquire_lock` (fix-PRD FR-1). On inner success return the lock unchanged
      (FR-3); on `LockError::IoError` fall back to the process-internal lock
      (FR-4); on `LockError::LockBusy` fall back **only if** `flock_support_for_dir`
      says unsupported, otherwise propagate `LockBusy` (FR-5).
- [x] 1.6a Implement fix-PRD **FR-16**: once `flock_support_for_dir` has
      classified a directory as unsupported, skip the inner `acquire_lock`
      **entirely** and go straight to the process-internal lock. This avoids a
      failing syscall on every reader reload and avoids creating a
      `.tantivy-meta.lock` that can never serve its purpose on that volume
      (`MmapDirectory` creates lock files and never deletes them). Without this
      and 1.5b, phase 2 would have to edit this file after all — defeating the
      "final location, unchanged" premise of task 1.0.
- [x] 1.6b Record the **third `IoError` source** at the FR-4 fallback:
      `MmapDirectory::acquire_lock` opens the lock file *before* locking it, so a
      read-only or unwritable directory fails at `open_write` and also produces
      `LockError::IoError` — which the fallback would silently turn into a
      *successful* process-internal lock. Keep the classified errno available on
      the fallback path so a caller can tell "fell back after an
      unsupported-operation errno" from "fell back after some other `IoError`";
      section E (task 4.2) reports exactly that distinction, and without it a
      diagnostic run on a genuinely broken volume reads as "the fix works"
      (review finding 18).
- [x] 1.7 Write the single-process invariant as a comment at the fallback: only
      one Simsapa process ever touches an index directory, because the searcher is
      the process-global `FULLTEXT_SEARCHER` (`lib.rs:169`) shared by the embedded
      webserver (fix-PRD FR-10). In the same comment, record **why the trait's
      default `acquire_lock` is not used** as the fallback: it locks by file
      existence and deletes the file on drop, but `MmapDirectory` never deletes
      its lock files (`ReleaseLockFile`'s `Drop` only closes the fd — the reason
      FR-23 expects stale `.tantivy-meta.lock` files), so it would find the
      leftover and return `LockBusy` forever. See review finding 2.
- [x] 1.8 Do **not** change any existing call site. `searcher.rs` and `indexer.rs`
      keep using bare `MmapDirectory` in this phase (PRD Non-Goals) — the wrapper
      is referenced only from the diagnostic.
- [x] 1.9 Add unit tests: the fallback guard actually excludes two concurrent
      acquisitions of the same path; two different paths do not block each other;
      `Supported`/`Busy`/`Unsupported` classification from synthesised errnos; the
      probe file is gone afterwards. `cd backend && cargo test` passes.

---

### 2.0 Section A + B: storage-location facts, the mount-table parser, and the primitive probes

**Specs to keep in mind.** FR-12 is the trap that produced the original relocated-
storage bug: the **recorded** path (from `storage-path.txt`, via
`storage_path_state().1`) and the **resolved** path (which silently falls back to
the internal app root) are different notions and must be printed separately, with
an explicit note when they differ — see `docs/relocated-storage-recovery.md`. The
mmap probe (FR-18) is the go/no-go measurement for the whole PRD: it must read
**three** bytes — first, middle, last — because a `direct_io` FUSE mount fails
only past page 0. Every probe reports elapsed time (FR-21) and cleans up via a
`Drop` guard (FR-39). No probe failure may abort the run (FR-44).

**Depends on:** 1.0 (reuses `probe_flock_support`).
**Blocks:** 5.0 (the report builder consumes these result structs).

- [x] 2.1 Create `backend/src/storage_diagnostics.rs`, declare it in
      `backend/src/lib.rs`, and define the result types the sections produce
      (`StorageLocationInfo`, `ProbeResults`, each field carrying its own
      `Duration` and an `Option<String>` error rather than returning `Result` from
      the section as a whole).
- [x] 2.2 Write the pure mount-table parser:
      `parse_mount_table(contents: &str, path: &Path) -> Option<MountEntry>`
      returning the mount point, filesystem type and mount options for the
      **longest mount point that prefixes** `path` (FR-15). Keep it free of I/O so
      it is fixture-testable off-device.
- [x] 2.3 Add the caller that reads `/proc/mounts` (guarded by `try_exists()`) and
      the `statfs` `f_type` numeric fallback for platforms without `/proc`, with a
      small lookup of well-known magics (ext4, F2FS, exFAT/vfat, FUSE, tmpfs) and
      the raw hex when unknown.
- [x] 2.4 Cover FR-14 and FR-16 (review findings 7 and 11). **Free/total space
      needs no new platform code and no `libc`:** `fs4` — already a direct
      dependency from task 1.1 — exports `fs4::statvfs(path)`,
      `fs4::available_space`, `fs4::free_space` and `fs4::total_space`
      (`fs4-0.13.1/src/lib.rs:199-226`), implemented over rustix on unix/Android
      (`src/unix.rs:75`) and `GetDiskFreeSpaceExW` on Windows
      (`src/windows.rs:118`). Do **not** write per-`cfg` `statvfs` helpers and do
      **not** use `sysinfo` (rejected for Android at `app_data.rs:5017`). For
      FR-16, derive internal-vs-external from the resolved path against the
      internal app root, not from the C++/JNI enumeration, which the backend
      cannot see.
- [x] 2.5 Build section A: recorded path, resolved path, an explicit "these
      differ" note, the `storage_path_state()` verdict (FR-13), free/total space
      on the volume (FR-14), the filesystem type + mount options (FR-15), and
      whether the location is internal or external (FR-16).
- [x] 2.5a Handle the **desktop case** of FR-13 honestly (review finding 13).
      `storage_path_state()` short-circuits to `(Absent, None)` whenever
      `!is_mobile()` (`lib.rs:1080-1082`), so on a healthy desktop install the
      raw verdict is `absent` with no recorded path. Section A must print that as
      "no recorded storage path — this is a mobile-only setting" rather than a
      bare `absent`, and must carry a flag the verdict deriver can read so
      task 5.4 does not fire the "storage location is unreachable" branch on
      every desktop run (FR-34, FR-37).
- [x] 2.6 Section B `flock` probe: call `probe_flock_support()` from task 1.0
      against an index directory and format the outcome as
      `supported` / `busy` / `unsupported(<errno> <NAME>)` / `error(<errno>)`,
      always printing the raw errno (FR-17). Reuse `probe_flock_support()`'s own
      `Drop` guard from task 1.3 — do **not** add a second cleanup guard for the
      same file.
- [x] 2.7 Section B **mmap probe** (FR-18): memory-map an existing index **segment
      file** read-only and read the first, a middle and the last byte, reporting
      success or the exact errno. Select the file **by size, not by extension**
      (review finding 21): take the largest regular file of at least ~8 KiB,
      skipping `meta.json`, `.managed.json`, `VERSION` and `*.lock`. An
      extension allowlist would happily pick a 200-byte `.fast` file, which
      cannot fault past page 0 — defeating the entire point of the middle and
      last reads. If no file meets the size floor, write the diagnostic's own
      probe file large enough to span several pages, mmap that instead, and say
      so explicitly in the output.
- [x] 2.8 Section B atomic-write probe (FR-19): write a temp file in the
      directory, `sync_data()`, `persist()` (rename) it over an existing target,
      then delete it — mirroring `MmapDirectory::atomic_write`
      (`mmap_directory.rs:352`).
- [x] 2.9 Section B plain read/write probe (FR-20): create, write, `fsync`,
      re-read and delete a small file, so "the volume is broken" is separable from
      "the volume lacks one primitive".
- [x] 2.10 Wrap every probe file in a `Drop` guard with a distinctive
      `simsapa-…` name (FR-39, §7), and give every probe an elapsed-time
      measurement (FR-21). Each probe file gets **exactly one** guard — the
      `flock` probe's already lives in task 1.3, so 2.6 reuses it rather than
      wrapping the same path twice. Verify by test that no probe file remains
      after a run, including a run where a probe returns an error.
- [x] 2.11 Ensure each probe catches its own error and continues — a failing
      probe records its error string and the section proceeds (FR-44). Add a test
      driving the section against a non-existent directory and asserting the run
      completes with populated error strings.

---

### 3.0 Sections C + D: index inventory, and the current Tantivy open sequence with per-step attribution

**Specs to keep in mind.** Section D reproduces what the app does **today** —
`MmapDirectory::open` → `Index::open` → `index.reader()` — and must attribute the
failure to the exact step, because the existing log conflates all three behind one
message (`searcher.rs:121`). The research predicts `Index::open` succeeds (it takes
no lock) and `index.reader()` is the failing step; the diagnostic must be able to
**disprove** that, so do not shortcut it. FR-40 is absolute: `Index::open`, never
`Index::open_or_create` — even though `searcher.rs:136` currently uses the latter.
FR-41: never touch `FULLTEXT_SEARCHER`; open independent instances.

**Depends on:** 2.1 (module + result types). **Blocks:** 4.0, 5.0.

- [x] 3.1 Enumerate the per-language index directories under
      `suttas/`, `dict_words/` and `library/` from `AppGlobalPaths`
      (`lib.rs:578-580`), tolerating a missing base directory without error.
- [x] 3.2 Section C per-directory row (one line each, per §6): area, language key,
      file count, total size, and whether `meta.json` is present and parseable
      (FR-22).
- [x] 3.3 Section C: report the presence and **age** of any
      `.tantivy-meta.lock` / `.tantivy-writer.lock` files, with wording that makes
      clear these are expected leftovers, not a fault — `MmapDirectory` creates
      them and never deletes them (FR-23). Both files are present in the shipped
      tree, so their absence (not their presence) is the unusual reading.
- [x] 3.3a **Collect 3.3's reading before section D executes**, and keep the
      snapshot. Section D's `index.reader()` reaches
      `MmapDirectory::acquire_lock`, which *opens — and therefore creates —* the
      lock file before locking it, so on an index directory that has never opened
      successfully the diagnostic writes lock files that were not there before.
      The FR-39 `Drop` guards do not remove them (they cover only `simsapa-…`
      names), and they are deliberately not deleted (they are zero-length,
      tantivy never deletes them, and any app startup creates them). Where the
      pre-run snapshot said *absent* and a post-run check finds one, print a
      single line saying the run created it — otherwise the report misstates the
      volume's prior state (FR-38a, finding 28). Add a test over a temp index
      directory asserting the pre-run snapshot is what section C reports.
- [x] 3.4 Section C: report the index `VERSION` **once, as a section-C header
      line — it is a single top-level file** (`<app-assets>/index/VERSION`,
      written by `write_version_file(&paths.index_dir)` at `indexer.rs:599`,
      contents `1.0`), **not** one per language directory, which is where an
      earlier draft of this task put it (finding 23). Use the existing public
      helpers `read_version_file()` / `is_index_current()` and the
      `INDEX_VERSION` constant (`indexer.rs:609-645`, already used by
      `sutta_bridge.rs:3856`) rather than re-reading the file by hand. Print the
      value **and** whether it matches `INDEX_VERSION`; a mismatch is a real
      "index stale or incomplete" input for the FR-34 verdict, which otherwise
      has no way to see it. Absence is a reported fact, not an error.
- [x] 3.5 Section D: for each index directory run the three-step current open
      sequence — `MmapDirectory::open` → `Index::open` → `index.reader()` —
      recording per step: ok / failed-with-full-error, plus elapsed time (FR-25).
      **`Index::open(directory)` takes no schema** (`index/index.rs:510`); it reads
      the schema from `meta.json`. Do **not** call `register_tokenizers` here
      (finding 32): section D measures opens and never queries, so registration is
      dead work that implies a parity with section E which does not exist. It
      belongs in task 4.3 alone, where the `QueryParser` resolves
      `{lang}_stem` / `{lang}_normalize` off the `Index` at parse time (review
      finding 14).
- [x] 3.5a **Drop the section-D readers** as soon as each directory's three steps
      are recorded. Section D deliberately reproduces the app's bare
      `index.reader()`, i.e. the default `ReloadPolicy::OnCommitWithDelay`, which
      spawns a 500 ms `meta.json`-polling thread per index that lives as long as
      the reader (research §8). Leaking six watcher threads per diagnostic run
      would contradict Goal 4 and success metric 6 ("no other app behaviour
      changes"). Add a test asserting the run leaves no additional live readers.
- [x] 3.6 Make the step attribution explicit in the output — one labelled line per
      step, so a reader can see which of the three failed without inference
      (FR-26). Word the `Index::open` line so the divergence from the app is
      visible: the app calls `open_or_create` and would silently *create* an empty
      index here, whereas the diagnostic only opens. A failure at this step means
      the index is missing or incomplete — not a fault the app exhibits at this
      step (review finding 5).
- [x] 3.7 Assert in code review and in a test that the diagnostic path contains no
      `open_or_create` call anywhere (FR-40), and that it constructs its own
      `Directory`/`Index` values without reading or replacing `FULLTEXT_SEARCHER`
      (FR-41).

---

### 4.0 Section E: open each index through the wrapper and run the hard-coded query

**Specs to keep in mind.** This section is the point of the exercise: a non-zero
hit count from a user's SD card is direct proof the phase-2 fix works on real
hardware, obtained before we commit to it (FR-29). It must use the **real**
wrapper from task 1.0 — same module, same fallback behaviour — not a
diagnostic-only approximation (FR-30), or it proves nothing. `ReloadPolicy::Manual`
here only (Non-Goals: the real search path is untouched). Query terms are
**hard-coded**, no input field (FR-28): a Pāli term (`nirodha`) and its English
translation (`cessation`), **both run against every index** (FR-28d) rather than
routed by language — see finding 26 for why routing produced a false fault on
`suttas/san`.

**Depends on:** 1.0 (the wrapper), 3.0 (the enumeration and schema selection).
**Blocks:** 5.0 (the verdict reads section E's outcome).

- [ ] 4.1 Repeat the three-step open per index directory, substituting
      `LenientLockMmapDirectory` for `MmapDirectory` and building the reader with
      `ReloadPolicy::Manual`; report success or failure **per step**, with the full
      error and elapsed time (FR-27).
- [ ] 4.2 Record which lock path the wrapper actually took for that directory —
      **three** outcomes, not two: inner `flock` succeeded; fell back after an
      unsupported-operation errno; fell back after some *other* `IoError`. Read
      them with **`LenientLockMmapDirectory::lock_paths()`**, which returns every
      distinct route in the order first seen, **not** `last_lock_path()`: one
      `index.reader()` acquires `META_LOCK` several times, so the last route can
      hide an earlier differing one (finding 34). The
      third matters because `MmapDirectory::acquire_lock` opens the lock file
      before locking it, so an unwritable directory also yields `IoError` and the
      fallback would report a clean success on a genuinely broken volume (task
      1.6b, review finding 18). State in the output that
      `ReloadPolicy::Manual` is **not** what makes this work:
      `open_segment_readers()` takes `META_LOCK` unconditionally on every reader
      build whatever the policy (`reader/mod.rs:194`), so a section E success is
      attributable to the wrapper alone (review finding 3).
- [ ] 4.3 On a successful open, call `register_tokenizers(&index, lang)`
      (`search/tokenizer.rs:186`, already `pub`) — **before** the `QueryParser` is
      constructed, not merely before the query runs (review finding 14) — then run
      the hard-coded queries and report the **hit count and elapsed milliseconds**
      for each (FR-28). Run **both terms — `nirodha` and `cessation` — against
      every index**, and report both counts on the row
      (`nirodha=N cessation=M`); do **not** route the term by index language
      (FR-28d, finding 26). The earlier per-language rule sent `cessation` at
      `suttas/san`, whose content is romanized Sanskrit, so a healthy populated
      index scored zero and landed in 4.4's *unexpected* bucket on every device
      with Sanskrit installed. Two queries per index costs nothing and deletes the
      classification question. Query the **`content`** field with a single
      `QueryParser::for_index(index, vec![content_field])`. All three schemas
      define `content` (`{lang}_stem`) and `content_exact` (`{lang}_normalize`) —
      `schema.rs:47-48` (suttas), `:99-100` (library), `:152-153` (dictionaries) —
      so one field selection works uniformly. Do **not** reproduce the live
      search's dual-field Must/Should boolean with its boost
      (`searcher.rs:515-525`): the diagnostic asks "can this index be read at
      all", and the extra machinery is more to get wrong for no diagnostic gain
      (review finding 14).
- [ ] 4.3a Handle a missing `content` field as an **attributed output line**, not
      an aborted section. `Index::open` reads the schema from `meta.json` rather
      than being handed one, so `index.schema().get_field("content")` can
      legitimately fail on a foreign, truncated or older `meta.json`. Print
      "schema has no `content` field" for that row and continue (FR-28a, FR-44,
      finding 31).
- [ ] 4.3b Report **`reader.searcher().num_docs()`** per index (FR-28c, finding
      27). Nothing else in sections A–F measures whether an index actually
      contains documents — C counts files, D and E measure opens — yet 4.4, 5.4,
      FR-28b and FR-37 all depend on exactly that. It is one line on a reader this
      section already holds, it turns the expected-vs-unexpected zero-hit split
      into a **measured** fact instead of an inference, and it independently proves
      the read path works even if both query terms come up empty.
- [ ] 4.4 Make a zero-hit success visibly distinct from an open failure in the
      output — the decision-gate table (PRD §9) branches on exactly that
      distinction. Drive the expected/unexpected split from **`num_docs`** (4.3b),
      not from a guess about the language: a zero hit against `num_docs == 0` is
      **expected** — the **library** index above all (most users import no books —
      review finding 9), but equally a `dict_words/<lang>` or `suttas/<lang>`
      index for a language whose content was never downloaded. The unexpected case
      is both terms scoring zero against an index with `num_docs > 0`.
- [ ] 4.5 Confirm the wrapper is imported from
      `backend/src/search/lenient_directory.rs` and that no parallel
      diagnostic-only copy of the lock logic exists anywhere (FR-30).

---

### 5.0 Sections F + G and the report builder

**Specs to keep in mind.** FR-33..37 govern the verdict: two or three plain
sentences at the **top**, derived from the measured results and never guessed,
covering healthy / unsupported-primitive (and whether section E handled it) /
missing-or-incomplete index / unreachable storage. An unrecognised pattern must
say plainly that it is unrecognised and ask the user to send the summary — never
invent a diagnosis (FR-35). The verdict must not name `flock`, `mmap`, `Tantivy`,
`FUSE` or errnos (FR-36), and must not imply a fault when the checks pass (FR-37).
FR-42: no sutta or dictionary content, no API keys, no bookmarks or history in the
output; paths and volume UUIDs are expected and fine.

**Depends on:** 2.0, 3.0, 4.0. **Blocks:** 6.0.

- [ ] 5.1 Add a diagnostics-only process-global record of per-index-directory open
      failures, populated where `searcher.rs:120` currently only calls `warn()`
      (path + error string). This changes no behaviour; it makes FR-31 answerable.
      Clear it on **every** searcher (re)open, or an entry recorded before a
      storage recovery survives and section F reports a failure that no longer
      exists (review finding 19). Add a test: record a failure, reopen, assert the
      record is empty.
- [ ] 5.1a Put the clear in a **small helper called by both constructors**.
      `FulltextSearcher` has two — `open()` (`searcher.rs:55`) and
      `open_from_dirs()` (`:78`) — and both call `open_indexes()` three times, so
      clearing only in `open()` leaves `open_from_dirs()` appending to a list that
      is never reset (FR-31a, finding 29). Cover `open_from_dirs()` in the 5.1
      test, not just `open()`.
- [ ] 5.2 Add a public accessor on `FulltextSearcher` returning the sutta / dict /
      library index counts, and read them through `with_fulltext_searcher()`
      (`lib.rs:369`) — a read-only borrow that never reinitialises the global
      (FR-31, FR-41). The existing `has_sutta_indexes()` / `has_dict_indexes()` /
      `has_library_indexes()` predicates (`:270`, `:275`, `:313`) answer a
      different question and are not a substitute.
- [ ] 5.2a Do **not** touch `is_fulltext_searcher_ready()` (`lib.rs:358`). It
      returns `true` whenever the global is `Some` regardless of index count, and
      that dishonesty is real — but correcting it is **phase-2 FR-20**, and doing
      it here changes `/health`'s `fulltext_searcher_ready` field, breaking Goal 4
      and success metric 6. Derive the "not initialised" state of 5.3 from
      `with_fulltext_searcher()` returning `None` instead (FR-31b, finding 30).
- [ ] 5.3 Section F: print those counts, the startup open failures, and the app
      version, platform and Android API level where applicable (FR-32). Print
      **"searcher not initialised this session"** as a state distinct from "0
      indexes": `init_fulltext_searcher()` is lazy and mode-gated (called from
      `run_search`), so on a healthy install where no Fulltext query has run,
      `FULLTEXT_SEARCHER` is legitimately `None` — and FR-41 forbids initialising
      it to find out (review finding 6). That state applies to the **startup
      failure list too**, not only the counts: the list of 5.1 is written while a
      searcher is being built, so an uninitialised searcher means "not measured",
      never "no failures" (review finding 19). Word both lines from the same
      state so they cannot disagree. Read the state from
      `with_fulltext_searcher()` returning `None`, **never** from
      `is_fulltext_searcher_ready()` (task 5.2a).
- [ ] 5.4 Implement `derive_verdict(&DiagnosticsResults) -> String` as a **pure**
      function over the collected result structs, covering the four named cases
      plus the explicit unrecognised-pattern fallback (FR-34, FR-35). **Four**
      inputs must **not** be treated as faults: an uninitialised searcher (5.3);
      a zero-hit from an index whose `num_docs` is 0 (4.3b/4.4 — this is now a
      measured input, not an inference); stale `.tantivy-*.lock` files (3.3); and
      a `StorageState::Absent` verdict on
      **desktop**, where `storage_path_state()` short-circuits to `(Absent, None)`
      for every install (`lib.rs:1080-1082`, task 2.5a, review finding 13) — left
      ungated, the "storage location is unreachable" branch would fire on every
      healthy desktop run. Each is normal, and FR-37 forbids implying a fault that
      was not found. Unit-test the desktop case explicitly.
- [ ] 5.5 Enforce the verdict's vocabulary rules — no `flock`/`mmap`/`Tantivy`/
      `FUSE`/errno names (FR-36) — and add a unit test asserting the produced
      verdict strings contain none of those tokens for every case.
- [ ] 5.6 Assemble the report: verdict first, then sections A–F with plain-text
      headers, per-section elapsed times, one line per index row (§6). Plain text
      only, no markup, safe to paste into an email. The **execution** order must
      match the report order at least to the extent that section C's lock-file
      snapshot is taken before section D runs (task 3.3a) — section D creates
      lock files, so a lazily-evaluated section C would report the diagnostic's
      own leftovers as pre-existing (FR-38a).
- [ ] 5.7 Write the complete summary to the log at INFO through the Rust logger,
      so `log.txt` alone is sufficient (FR-11). Log it in one call, or in clearly
      contiguous lines that reassemble.
- [ ] 5.8 Expose the entry point `pub fn run_storage_diagnostics() -> String`
      (FR-45), and review the assembled output against FR-42 — assert in a test
      that it contains no `api_key`-shaped content and no document text.
- [ ] 5.9 Unit-test the report builder end-to-end against fixture result structs
      for: healthy, `flock`-unsupported-but-section-E-succeeded,
      `flock`-unsupported-and-mmap-failed, missing index, unreachable storage,
      **healthy-on-desktop** (the `Absent`-but-not-a-fault case of 5.4), and
      **empty-but-healthy** — an index with `num_docs == 0` and zero hits for both
      terms, which must produce no fault (4.4, FR-28b). Add a regression fixture
      for the case finding 26 describes: a populated non-Pāli, non-English index
      (`suttas/san`, `num_docs > 0`, `nirodha` hits, `cessation` does not) must
      read as healthy.

---

### 6.0 Bridge wiring: an off-thread invokable with a completion signal

**Specs to keep in mind.** CXX-Qt invokables run on the **calling (QML) thread**,
so the run must be spawned (FR-4); the model is
`sutta_bridge.rs:3879` (`rebuild_search_index`) with its
`rebuildSearchIndexCompleted(bool, QString)` signal at `sutta_bridge.rs:819`, and
`storage_manager.rs:138-168` for the generation-counter variant. Any new
`SuttaBridge` method needs a matching `qmllint` stub (FR-47).

**Depends on:** 5.0. **Blocks:** 7.0.

- [ ] 6.1 Add `#[qsignal] storageDiagnosticsCompleted(success: bool, summary: QString)`
      to `bridges/src/sutta_bridge.rs`, following the existing `#[cxx_name = …]`
      convention.
- [ ] 6.2 Add `#[qinvokable] run_storage_diagnostics(self: Pin<&mut SuttaBridge>)`
      that captures `qt_thread()`, spawns a thread, calls the backend's
      `run_storage_diagnostics()`, and queues the signal back with the summary.
      Catch a panic in the worker (`catch_unwind`) and emit `success: false` with
      the panic message rather than losing the signal (FR-44).
- [ ] 6.3 Add matching stubs to
      `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` — the method with a
      trivial body and the signal declaration — for `qmllint` (FR-47). The
      neighbouring code **is** the model to copy (finding 22, correcting review
      finding 20, which claimed the opposite): the file already has a "Search
      index signals" group with `rebuildSearchIndexProgress` (`:45`) and
      `rebuildSearchIndexCompleted` (`:46`); the signal block runs to `:51`, and
      the `rebuild_search_index()` function stub is at `:264`. Add
      `signal storageDiagnosticsCompleted(success: bool, summary: string);` to
      that same group and the function stub beside `rebuild_search_index()`.
- [ ] 6.4 Build with `make build -B` and confirm the bridge compiles and the
      generated QML type exposes the method and signal.

---

### 7.0 QML: the results dialog and the two entry points

**Specs to keep in mind.** Both entry points are **`ApplicationWindow` roots**
(not `Dialog`s), declared as inline siblings of `SuttaSearchWindow.qml` —
`AboutDialog` at `:2257`, `DatabaseValidationDialog` at `:2277`. The results
dialog matches them: an `ApplicationWindow` root, declared **once** in
`SuttaSearchWindow.qml` as a third sibling that both entry points open (review
finding 16). That fixes which rules of
`docs/android-edge-to-edge-and-safe-areas.md` apply: **never assign
`topPadding`/`padding` on the root**, and **anchor the root content item to its
parent rather than sizing it from `root.width`/`root.height`**. The `Popup`-family
padding rule does *not* apply here. Per
`docs/startup-sequence-and-caches.md`, an `ApplicationWindow` root cannot be
wrapped in a `Loader` — deferring it needs `Component` + `createObject` — and an
eagerly-created sibling runs its `Component.onCompleted` inside the engine load,
before `app.exec()`, so keep it trivial or defer it. FR-5:
bracket the run with `AssetManager.set_keep_screen_on(true)`/`(false)`, released
on **both** success and failure — and, because the completion signal is global,
guard the handler with an "initiated here" boolean so only the window that started
the run reacts and releases its own lock (CLAUDE.md). Button labels are exactly
**"Run Storage Diagnostics"**, **"Copy"**, **"Close"** (§6). QML logging uses
`Logger` with a single concatenated string (FR-48).

**Two things the earlier draft missed** (findings 24, 25). `DatabaseValidationDialog`
is **`modality: Qt.ApplicationModal`** (`:18`) while `AboutDialog` sets no
modality, so a non-modal results window opens *dead to clicks* from the Database
Validation entry point and works fine from About — which is the entry point
implemented first, so the bug hides. And every dialog sibling declares
**`required property int extra_top_margin`**, bound from `SuttaSearchWindow.qml`;
leaving it unbound is a **runtime** QML failure that a green `make build -B` will
not catch.

**One owner for the run** (FR-10c). The results window owns `open_and_run()`, the
`Connections`, the "initiated here" flag, the busy state and the `AssetManager`
bracket. The entry-point dialogs only call `open_and_run()`. An earlier draft put
the bracket and the flag in *both entry points* while the handler lived in the
dialog — three objects on one process-global signal, with the flag in the wrong
two.

**Depends on:** 6.0. **Blocks:** 8.0.

- [ ] 7.1 Create `assets/qml/StorageDiagnosticsDialog.qml` — an
      **`ApplicationWindow`** root matching its two siblings — with a monospace,
      **selectable**, scrollable text area for the summary (FR-6), and add it to
      the `qml_files` list in `bridges/build.rs` (FR-47).
- [ ] 7.1a Declare **one** instance in `assets/qml/SuttaSearchWindow.qml`
      alongside `AboutDialog` (`:2257`) and `DatabaseValidationDialog` (`:2277`),
      and give it a small API (e.g. `open_and_run()`) that both entry points call.
      Keep its `Component.onCompleted` trivial, or create it lazily with
      `Component` + `createObject`, because an eager sibling is constructed during
      the engine load before `app.exec()` — see §6 of
      `docs/startup-sequence-and-caches.md`. Do **not** wrap it in a `Loader`: the
      root is an `ApplicationWindow`.
- [ ] 7.1b Set **`modality: Qt.ApplicationModal`** on the root (FR-10a, finding
      24). `DatabaseValidationDialog.qml:18` is `Qt.ApplicationModal`, so a
      non-modal results window opened from it is input-blocked while that modal
      stays open — the window appears and does not respond. A modal shown *later*
      heads the modal stack and is not itself blocked, so matching the modality is
      the fix. `AboutDialog` sets no modality, so **verify from the Database
      Validation entry point specifically** — the About path cannot reveal this.
- [ ] 7.1c Declare `required property int extra_top_margin` on the root and bind
      it in `SuttaSearchWindow.qml` as `extra_top_margin: root.extra_top_margin`,
      matching every sibling at `:2257-2295` (FR-10b, finding 25). An unbound
      `required property` fails at **instantiation**, not at build, so
      `make build -B` passing proves nothing here.
- [ ] 7.2 Add the **Copy** button reusing the invisible-`TextEdit`
      `clipboard_helper` pattern (`AboutDialog.qml:63-71`), copying the entire
      summary (FR-7), with a brief "Copied" label confirmation that reverts after a
      moment (§6).
- [ ] 7.3 Add the **Close** button (FR-8) and the instruction line telling the user
      to send both the copied text **and** their `log.txt`, noting that `log.txt`
      can be saved or copied from the log-file list in the About dialog (FR-9).
- [ ] 7.4 Apply the `ApplicationWindow` rules of FR-10 — and only those, since
      the root is not a `Popup`: the root content item is anchored to its parent
      (which Qt has already reparented to the inset `contentItem`), never sized
      from `root.width`/`root.height`, and **no** `topPadding` or `padding` is
      assigned on the root, which would silently replace Qt's safe-area binding.
      Sizing the *window* itself is fine and expected — `DatabaseValidationDialog.qml:12-13`
      (`is_mobile ? Screen.desktopAvailableWidth : 600`) is the shape to copy.
- [ ] 7.5 Show a busy indicator while the run is in flight and disable the trigger
      button (FR-4); connect `SuttaBridge.storageDiagnosticsCompleted` **in the
      results dialog** to populate the text area and re-enable. This dialog is the
      single owner of the run state (FR-10c): the `Connections`, the "initiated
      here" guard and the busy flag all live here, not in the entry points.
- [ ] 7.6 Add the **"Run Storage Diagnostics"** button to the bottom button row of
      `assets/qml/AboutDialog.qml`, alongside "Copy App Info" and "Close"
      (FR-1), on **all** platforms with no mobile-only gate (FR-3).
- [ ] 7.7 Add the same action to `assets/qml/DatabaseValidationDialog.qml`
      (FR-2), opening the **single** instance declared in `SuttaSearchWindow.qml`
      by task 7.1a rather than declaring a second one.
- [ ] 7.8 Bracket the run with `AssetManager.set_keep_screen_on(true)` /
      `(false)` **in the results dialog** — the single owner (FR-10c) — releasing
      in the completion handler on success **and** failure, guarded by the
      "initiated here" boolean (FR-5). Give the results dialog its own
      `AssetManager { id: manager }`, copying the shape of
      `DatabaseValidationDialog.qml:297` with its bracket at `:305`/`:326`.
      Neither entry point needs an `AssetManager` for this, so **do not** add one
      to `AboutDialog.qml` — an earlier draft of this task did, which would have
      split the bracket and the guard across three objects listening to one
      process-global signal.
- [ ] 7.9 Run `make qml-test`; confirm no `console.*` calls were introduced
      (FR-48) and `qmllint` is clean.

---

### 8.0 Tests, documentation, and verification

**Specs to keep in mind.** Success metric 7 requires `cargo test` and
`make qml-test` to pass, with unit tests for the report builder and the mount-table
parser using fixture input. Success metric 6 requires that on a healthy desktop
install nothing else about the app behaves differently — the only intended
behaviour change in this whole PRD is a new button (Goal 4).

**Depends on:** 1.0–7.0.

- [ ] 8.1 Consolidate the unit tests: mount-table parser fixtures (Android
      `/proc/mounts` with sdcardfs + FUSE lines, a plain Linux one), the verdict
      deriver's cases — including the two not-a-fault regressions of 5.9
      (`num_docs == 0`, and a populated `suttas/san` where only `nirodha` hits) —
      and the report builder's end-to-end fixture output.
- [ ] 8.2 Add the probe-cleanup regression test: after a full run against a temp
      directory, no `simsapa-*` probe file remains (FR-39, metric 5) — including on
      the error path.
- [ ] 8.3 Confirm the report has **exactly one caller** — the `SuttaBridge`
      invokable behind the UI button. No CLI subcommand, no `/health` field, no
      other route (PRD Non-Goals, §11.2): the affected users are reached by asking
      them to press the button, and `/health` would be actively wrong for a report
      that writes probe files and opens every index. Document at
      `run_storage_diagnostics()` that it depends on `get_app_globals()` — which
      **panics** when uninitialised (`lib.rs:180`) — and that the GUI satisfies
      that because `gui.cpp:403` initialises unconditionally before any dialog
      exists (review finding 8), so a future headless caller does not inherit the
      assumption silently.
- [ ] 8.4 Write `docs/storage-diagnostics.md`: what each section measures and why,
      how to read the report, the PRD §9 decision-gate table, and the note that the
      wrapper here is phase-2 code wired only into the diagnostic.
- [ ] 8.5 Update `CLAUDE.md`'s notable-docs list and `PROJECT_MAP.md` with the new
      modules and doc.
- [ ] 8.6 Run `cd backend && cargo test`, `make qml-test` and `make build -B`;
      record any pre-existing timing-assertion drift separately rather than as a
      regression.
- [ ] 8.7 Re-read the PRD's Non-Goals and diff the branch: confirm no existing call
      site was switched to the wrapper, no `ReloadPolicy` change reached the real
      search path, no auto-run of the diagnostics exists, no network call was
      added, the diagnostic leaves no live readers or watcher threads behind
      (task 3.5a), and **`is_fulltext_searcher_ready()` is unchanged** (task 5.2a
      — it is phase-2 FR-20's to fix, and touching it here silently alters
      `/health`).
- [ ] 8.7a Verify the two entry points **separately on a running desktop build**,
      Database Validation first: the results window must be interactive when
      opened from the `Qt.ApplicationModal` Database Validation dialog (task
      7.1b), and must instantiate at all — an unbound `extra_top_margin` fails
      only at runtime (task 7.1c). Neither defect is visible from the About path
      or from a green build. (Per CLAUDE.md this is a **manual user check**, not
      an agent-run GUI test.)
- [ ] 8.8 Add any question discovered during implementation to PRD §11 rather than
      resolving it silently (PRD §11.1).
