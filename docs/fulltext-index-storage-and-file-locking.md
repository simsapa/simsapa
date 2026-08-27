# Fulltext index storage and file locking

The fulltext (Tantivy) index has to open on storage that does not implement
`flock(2)`. ChromeOS/ARCVM's `fuse`-backed external volumes and portable SD
cards are two instances of that one class — the issue is the **filesystem's
locking support**, not the hardware. `backend/src/search/lenient_directory.rs`
(`LenientLockMmapDirectory`) is the wrapper that makes those volumes work, and
it is wired into every real search and index path.

Source PRDs:
`tasks/2026-08-05-193727-prd---fulltext-index-on-filesystems-without-flock.md`
(the fix) and
`tasks/2026-08-05-201545-prd---run-storage-diagnostics.md` (the phase-1
diagnostic whose §12 holds the measurements this rests on).

## 1. `flock` vs `fcntl` — why everything else passed

The symptom was the worst kind: on the affected volume **every fulltext search
returned zero results, silently**, while ContainsMatch worked, the SQLite
databases worked, first-run setup completed, and **Database Validation reported
no errors**. Nothing in the app said anything was wrong.

There are two unrelated POSIX advisory-locking mechanisms, and the split between
them is exactly the split between what worked and what did not:

| Mechanism | Who uses it | On the affected volume |
|---|---|---|
| `fcntl(F_SETLK)` byte-range record locks | **SQLite** | works |
| `flock(2)` whole-file locks | **Tantivy**, via `fs4`'s `lock_exclusive()` / `try_lock_exclusive()` | **`ENOSYS` (errno 38)** |

Android mounts a *portable* (non-adopted) SD card, and ChromeOS/ARCVM mounts an
external volume, through a **FUSE layer** that does not implement `flock`. The
call does not fail slowly or partially — it returns "function not implemented".

So every existing check passed. The tier-2 storage probe
(`backend/src/storage_probe.rs`) writes and exercises a **real SQLite database**,
which is `fcntl`; the volume passes it, passes Database Validation, and passes
every first-run check. There was no code path anywhere in the app that exercised
`flock` before the index tried to open — which is why this reached a user.

**The lesson is more general than this bug: a probe proves only the primitive it
actually used.** If a future feature depends on a filesystem capability, probe
*that* capability. `storage_probe.rs` now also records the `flock` verdict for
exactly this reason (see §6).

## 2. The two Tantivy lock sites, and why their failures look different

Verified against tantivy 0.25.0. These are the **only two** lock sites in the
crate:

| Site | Lock | Blocking? | What a failure looks like |
|---|---|---|---|
| `reader/mod.rs:194` — `IndexReader` build and every reload | `META_LOCK` | **yes** | `LockError::IoError(…)` — **the real errno survives** |
| `index/index.rs:545` — `Index::writer()` | `INDEX_WRITER_LOCK` | no | `LockError::LockBusy` — `try_lock_exclusive().map_err(\|_\| LockBusy)` **discards the errno** |

Two consequences, both load-bearing:

- **The read path is what broke.** `META_LOCK` is taken *unconditionally* when a
  reader is built, so `index.reader()` failed for every index — for **reading**,
  not just writing. The user's log showed exactly that: `Failed to acquire
  Lockfile: IoError(Os { code: 38, kind: Unsupported, … })`, six times, followed
  by `FulltextSearcher opened: 0 sutta …, 0 dict …, 0 library …`.
- **On the write path the error type alone cannot tell you anything.**
  `LockBusy` conflates "another writer holds it" with "this filesystem does not
  do locking". That is why the wrapper needs a **probe** (§4) rather than just an
  error-code check: without it, masking `LockBusy` would let two writers into one
  index on a volume where locking works fine.

**`Index::open` and `Index::open_or_create` take no lock at all** — they are
`ManagedDirectory::wrap` + `load_metas`. The phase-1 diagnostic confirmed this on
the device: `MmapDirectory::open` **ok** → `Index::open` **ok** →
`index.reader()` **FAILED**. So the failure is precisely and only at the reader.

## 3. `LenientLockMmapDirectory` — the wrapper

`backend/src/search/lenient_directory.rs`. It wraps `MmapDirectory` and
**delegates every trait method unchanged except `acquire_lock`**.

The delegation is total by construction: `MmapDirectory`'s `impl Directory`
defines nine methods, all *required* (the trait has no defaults for them), so a
future tantivy that adds one **breaks the build loudly** rather than
mis-delegating silently. Nothing in tantivy downcasts a `Box<dyn Directory>`
back to a concrete `MmapDirectory`, so no hidden fast path is lost by wrapping.

`acquire_lock` takes one of four routes, and **which route it took is recorded**
(`LockPathTaken`, readable via `lock_paths()`):

1. **The volume is already known unsupported** → skip the inner call entirely
   and take the fallback. Skipping matters: calling and discarding would cost a
   failing syscall on every reader reload *and* would create a
   `.tantivy-meta.lock` that can never work — `MmapDirectory` creates lock files
   and never deletes them (`ReleaseLockFile`'s `Drop` only closes the fd).
2. **Inner `flock` succeeded** → hand the guard back unchanged. On an ordinary
   filesystem this is the only route ever taken, and behaviour is bit-for-bit
   identical to before the wrapper existed.
3. **Inner call failed with `IoError`** → classify the errno, then fall back.
4. **Inner call failed with `LockBusy`** → **propagate it**. Route 1 has already
   returned for unsupported volumes, so reaching here means locking works and
   something genuinely holds the lock.

### Route 3 keeps two causes apart, and that is the point

`MmapDirectory::acquire_lock` *opens* the lock file before locking it, so a
**read-only or otherwise unwritable directory** fails at `open_write` and
produces `LockError::IoError` too. Falling back there would convert a genuinely
broken volume into an apparently successful lock — a diagnostic run on a broken
volume would read as *"the fix works"*.

So `LockPathTaken` distinguishes `FallbackUnsupported { errno, name }` from
`FallbackOtherIoError { message }`. On the reporting device **all six**
directories reported the former ("fell back after an unsupported-operation errno
(38 ENOSYS)"), which is what makes "the fallback is working around an
unsupported primitive" a measurement rather than a hope.

**Every distinct route is kept, not just the last.** One `index.reader()`
reaches `acquire_lock` more than once, so a single overwritten slot could hide an
early `FallbackOtherIoError` behind a later `InnerFlock`.

### The fallback lock is a real mutual exclusion

Not a no-op guard. Tantivy's garbage collector runs automatically after every
commit and every merge and takes `META_LOCK` precisely to avoid deleting segment
files a reader is opening — and this app keeps the previous searcher alive while
a rebuild or import runs. The race is real **in-process**.

Three implementation constraints worth not rediscovering:

- **The guard is hand-rolled, not a `MutexGuard`.** `DirectoryLock` is
  `Box<dyn Send + Sync + 'static>`, and both `std::sync::MutexGuard` and
  `parking_lot::MutexGuard` are `!Send`. `FallbackLock` is a flag plus a
  `Condvar`; `FallbackLockGuard::drop` releases it however the caller unwinds.
- **Blocking semantics are preserved per lock.** `META_LOCK` (`is_blocking:
  true`) blocks, bounded at 100 retries × 100 ms — mirroring tantivy's own
  `RetryPolicy` — and logs an error on exhaustion instead of freezing a thread.
  `INDEX_WRITER_LOCK` (`is_blocking: false`) does a `try_lock` and returns
  `LockBusy`, so the single-writer guarantee still holds in-process.
- **The `Directory` trait's own default `acquire_lock` is not usable as the
  fallback.** It locks by *file existence*, with the guard **deleting** the file
  on drop — but `MmapDirectory` never deletes its lock files, so stale
  `.tantivy-meta.lock` files are expected to be lying about in index
  directories. A file-existence fallback would find one and return `LockBusy`
  **forever**.

### The single-process invariant

**The fallback lock is process-internal only.** It is sufficient because only one
Simsapa process ever touches an index directory: the searcher is the
process-global `FULLTEXT_SEARCHER` in `backend/src/lib.rs`, and the embedded
webserver shares it. This is recorded as a comment at `acquire_fallback_lock` so
a future multi-process design does not silently inherit an unsafe lock. A
cross-process lock on a filesystem with no working `flock` is not solvable
in-app.

## 4. The probe, and its key normalisation

`probe_flock_support(dir)` creates `simsapa-flock-probe.tmp` in the directory,
attempts `try_lock_exclusive()`, classifies the errno, and deletes the file on
**every** exit path — success, failure and panic — via a `Drop` guard modelled on
`storage_probe.rs`'s. Leaving litter on a user's card is a user-visible defect.

`ENOSYS`, `EOPNOTSUPP`, `ENOTSUP` and `EINVAL` classify as *unsupported*. Two
notes: `EOPNOTSUPP` and `ENOTSUP` are the **same number (95)** on Linux and
Android, so the classification does not pretend to tell them apart; and `fs4`
0.13 is rustix-based and exports no errno constants, so `libc` supplies them. On
platforms that are neither unix nor Windows the classifier **never** answers
"unsupported" — a wrong `Unsupported` is cached for the process lifetime and
disables the inner lock, so the safe default is "some other error".

The verdict is cached **once per index directory** for the process lifetime
(`flock_support_for_dir`). It has to be: every reader build takes `META_LOCK`, so
a broken cache would turn a file create/lock/delete into a per-query cost. The
benchmark asserts this directly (§9).

**The cache key and the fallback lock table share one normalisation helper**
(`normalize_lock_key`), so they cannot disagree about what "the same directory"
means. It tries `fs::canonicalize`, and on failure falls back to
`std::path::absolute` + `crate::normalize_lexically` — never to skipping the
entry. Both halves are deliberate:

- `canonicalize()` **can fail on exactly the volumes this module exists for**;
  the storage code avoids it for that reason (`docs/relocated-storage-recovery.md`).
- Skipping would hand two `Directory` instances on one index directory **two
  different mutexes**, which is the precise bug the key exists to prevent.
- The lexical normalisation is needed because `std::path::absolute` deliberately
  keeps `..` components on unix, so two spellings of one path would otherwise
  produce two keys.

## 5. `mmap` works — the contingency that was measured away

The fix would have been **necessary but not sufficient** if the volume also
refused `mmap`: every index read goes through `memmap2::Mmap::map()`, and FUSE
volumes mounted with `direct_io` reject `mmap(MAP_SHARED)` with `ENODEV`. SQLite
working proves nothing here either — Android SQLite defaults to `mmap_size = 0`
and uses `pread`/`pwrite`.

So the phase-1 diagnostic shipped an `mmap` probe **before** the wrapper, and its
answer decided whether a whole second PRD (a `pread`-backed `Directory`) was
needed. It mapped a real 18,128,732-byte `.pos` segment file and forced reads at
offsets 0, 9,064,365 and **18,128,731** — faulting well past page 0, which is the
case a `direct_io` mount fails. It took 94.6 ms and **worked**.

**The non-mmap contingency is therefore not needed and must not be written.**
The probe file is chosen **by size, never by extension**, because only a fault
past page 0 catches this; see `docs/storage-diagnostics.md`.

The same run measured the wrapper end to end on the device: all six index
directories opened and returned real hits (`dict_words/pli`: 539,569 docs, 2,227
hits for *nirodha* in 328.6 ms). **The affected volume is not slow** — which
retired the SD-card performance notice the fix PRD had planned in §4.7 (there is
still no measurement of a slow affected volume, so the notice would assert
something unmeasured).

## 6. Honest reporting — the second half of the fix

Making search work is only half of it. In the same diagnostic session, the app
reported **0 / 0 / 0 indexes open with 6 failures** while the diagnostic opened
all six and searched them successfully — and the app called itself
*"initialized"* throughout. The user's searches had been silently empty for
weeks, and nothing in the UI said so.

`backend/src/fulltext_status.rs` is the single place that turns per-area open
counts plus recorded failures into one verdict and one plain-language sentence.
**Every user-facing string this feature can emit is written in that file**, which
is what lets its `no_jargon_in_user_facing_strings` test enforce the wording rule
across the whole feature at once.

`FulltextState` has **four** variants, not two, and the distinction is the
`StartupDbReport` principle applied here — "the files are not there" and "the
files are there and would not open" are different diagnoses and must never be
conflated:

| State | Means |
|---|---|
| `NotMeasured` | No searcher has been built this session. Not "no failures". |
| `FilesNotFound` | No index directory exists. Nothing is broken; offer a rebuild. |
| `CouldNotOpen` | Directories exist and not one index opened. **The state this whole fix exists for.** |
| `Ready` | At least one index is open and searchable. |

Two fields answer two different questions, and both are needed:

- **`state`** — can fulltext search return results at all?
- **`is_valid`** — did *everything that should have opened* open? False the
  moment any index directory fails. A partly-open index is both — search works,
  and something is wrong — and Database Validation must not print "All checks
  passed" over it.

`area_state()` / `area_message()` narrow the same question to one search area,
because neither "all three areas" nor "the whole area" is the right granularity:
a user whose sutta indexes open and whose dictionary indexes all fail was back to
a silent "No results found." on every dictionary search, and so was a user whose
`suttas/en` opened while `suttas/pli` failed. **An empty `area_message()` is the
whole instruction to stay silent**, so QML does not branch on the state at all
and the next case like this is a backend change only.

Where the verdict surfaces:

- **The search results empty state** (`bridges/assets/qml/FulltextResults.qml`) — gated
  on the *area* searched **and** on the search **mode**. The mode gate is an
  allowlist (`Fulltext Match`, `Combined`), not a denylist: Contains Match, Title
  Match, Headword Match and DPD Lookup all go through FTS5/SQLite and work
  perfectly on a volume where every Tantivy index failed, so blaming the index
  for one of those would be a fabricated diagnosis — the same dishonesty this
  work exists to remove, pointed the other way.
- **Database Validation** (`bridges/assets/qml/DatabaseValidationDialog.qml`) — a
  "Search index:" section fed by the existing `database_validation_result`
  signal, no new plumbing. It is deliberately kept **out of**
  `validation_results`: it is not in `expected_databases` (letting it in would
  break the three-database completion state machine) and it is not downloadable
  (it must never reach `get_failed_downloadable_list()`, which would build a
  bogus asset URL).
- **`/health`** — `fulltext_searcher_ready` is now false when zero indexes are
  open, plus the per-area counts, `failed`, `state` and `message`, built through
  the same helpers the app's own UI reads so the two cannot drift. See
  `docs/simsapa-localhost-api-search-endpoints.md`.
- **The log** — `reinit_fulltext_searcher()` logs the counts, at **ERROR** when
  it completes with zero indexes open. A log alone tells the story.
- **`storage_probe.rs`** — the tier-2 probe now runs the `flock` probe and logs
  the verdict as *"recorded only, never demotes"*. **A volume that fails only
  the `flock` test must not be demoted**: with the wrapper in place the app works
  on it. It does *not* run the `mmap` probe there, because that probe is only
  meaningful against a file large enough to fault past page 0, and writing an
  18 MB file to a card to earn one log line is not a reasonable price. Both
  verdicts are instead logged together, once per process, against the **real
  index directory** by `storage_diagnostics::log_storage_capability_verdicts()`,
  called from `reinit_fulltext_searcher()` — the honest place for it, since that
  is the directory the searcher is about to use.

### Wording rule

No `flock`, `ENOSYS`, `Tantivy`, `META_LOCK`, `mmap` or `FUSE` in any
user-facing string, and **never "SD card"** — say "this storage location". The
one device actually measured is a ChromeOS external volume, not a card. The
user-facing concepts are *"this storage location does not support the file
locking the search index needs; Simsapa is working around it"* and, in the
failure case, *"the search index could not be opened."*

## 7. Two dependencies this created

- **`ReloadPolicy::Manual` made `reinit_fulltext_searcher()` mandatory.** The
  default `OnCommitWithDelay` policy spawns a `meta.json`-polling thread per
  index; dropping it removed the 500 ms poll that one call site had been
  silently relying on. `DictionaryManager::start_reconcile()` — the same
  reconcile as the startup one, reached from the GUI — mutated the dictionary
  index and never reinitialised the searcher, so a GUI-triggered reconcile would
  have gone unnoticed by the open searcher for the rest of the session. **Every
  in-app index mutation must now be followed by an explicit
  `reinit_fulltext_searcher()`**; there are five such sites.
- **Opening the searcher is serialised** (`SEARCHER_OPEN_LOCK` in
  `backend/src/lib.rs`). `FulltextSearcher::begin_open_session()` **clears** the
  recorded failure list, so two concurrent openers can leave zero indexes open
  *and* zero failures recorded — which classifies as `FilesNotFound`
  ("index files not found") over a volume whose indexes are all present and all
  failed to open. `init_fulltext_searcher()` keeps a lock-free fast path but
  **re-checks under the lock**; without the second check the lock buys nothing.

## 8. The read path opens, it does not create — and what that gave up

`open_single_index` (`backend/src/search/searcher.rs`) uses `Index::open` when
the directory already holds an index, and falls back to `Index::open_or_create`
only when it does not. Creating an index from the *search* path is never
correct: it leaves an empty index behind and reports success. The write paths in
`indexer.rs` keep `open_or_create`, which is where creating one is the point.

**One thing was given up with it, and it is worth knowing about.**
`Index::open_or_create` compares the on-disk schema against the schema it was
handed and returns `SchemaError` when they differ; `Index::open` takes whatever
is on disk. So an index built by an older Simsapa whose schema has since changed
now opens *silently* instead of being recorded as an open failure, and the
mismatch surfaces later, per query, as a parse error against a field that is not
there.

Nothing is broken today, because a schema change is supposed to come with a
bump to `INDEX_VERSION` (`backend/src/search/indexer.rs`), which
`is_index_current()` reads and `SuttaBridge::check_search_index_status` reports
so the user is offered a rebuild. But that is now the **only** thing standing
between a schema change and a silently wrong index: **if you change any schema
in `backend/src/search/schema.rs`, bump `INDEX_VERSION` in the same commit.**
The type system will not remind you, and neither will Tantivy any more.

## 9. Benchmark — the wrapper costs nothing on a normal filesystem

`backend/tests/test_lenient_directory_benchmark.rs`.

On an ordinary desktop filesystem `probe_flock_support` answers `Supported` and
the wrapper delegates straight to `MmapDirectory`, so its only theoretical costs
are one probe per directory (cached for the process by `flock_support_for_dir`),
one enum comparison per `acquire_lock`, and one record pushed onto the route
log. The test measures that rather than asserting it: both arms run the
**identical** sequence — open → `Index::open` → `register_tokenizers` →
`reader()` with `ReloadPolicy::Manual` → the two Storage Diagnostics query terms
(`nirodha`, `cessation`) — against the **same** on-disk indexes, so the only
variable is the `Directory` implementation.

Three rules the test follows, each of which is a project constraint rather than
a preference:

- **Ratios between two arms measured in the same run**, never a millisecond
  budget. Absolute-time assertions in this repo are known to drift, and these
  figures move with page cache state and machine load. The limit is a generous
  1.25×, applied to the **aggregate** across all indexes: any one index's
  sub-millisecond reader build is noise, the sum is signal.
- **It enumerates what it finds.** The developer tree here has `suttas/hu` and
  no `suttas/san`; the reporting ChromeOS user's is the other way round. No
  language key is hard-coded.
- **It never creates an index**, and skips with a message when no index tree is
  present, so it is not a failure on a machine without one.

### Measured 2026-08-26 (`--release`, `--test-threads=1`)

Six iterations per index per arm, the first discarded (cold page cache, and the
one-time `flock` probe). `_b` is the bare `MmapDirectory`, `_w` the wrapper;
`open` covers directory open + `Index::open` + tokenizer registration, `search`
covers both query terms.

| index | num_docs | open_b_ms | open_w_ms | read_b_ms | read_w_ms | srch_b_ms | srch_w_ms | open× | read× | srch× |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| dict_words/en | 13587 | 0.043 | 0.040 | 0.107 | 0.105 | 0.035 | 0.033 | 0.95 | 0.99 | 0.95 |
| dict_words/pli | 539569 | 0.050 | 0.050 | 0.412 | 0.431 | 0.187 | 0.186 | 1.01 | 1.05 | 0.99 |
| library/en | 322 | 0.049 | 0.047 | 0.384 | 0.397 | 0.085 | 0.079 | 0.97 | 1.03 | 0.93 |
| suttas/en | 10649 | 0.048 | 0.047 | 0.323 | 0.321 | 0.081 | 0.077 | 0.99 | 0.99 | 0.94 |
| suttas/hu | 494 | 0.052 | 0.048 | 0.360 | 0.393 | 0.057 | 0.056 | 0.93 | 1.09 | 0.99 |
| suttas/pli | 10649 | 0.051 | 0.057 | 0.475 | 0.462 | 0.138 | 0.138 | 1.13 | 0.97 | 1.00 |

Aggregate over the six indexes: open 0.292 → 0.291 ms (**1.00×**), reader build
2.060 → 2.110 ms (**1.02×**), search 0.583 → 0.568 ms (**0.97×**). Every
per-index ratio is within noise of 1.0 in both directions, which is the shape a
no-cost result has. The debug profile reproduces the same conclusion (aggregate
0.97× / 1.00× / 0.97×), so the result is not an artefact of optimisation level.

Read the reader-build column against the pre-change baseline recorded in the
task list (0.61–0.81 ms per index) with care: that baseline used the **default**
reload policy, so it also paid for spawning a `meta.json` watcher thread. The
column above is `ReloadPolicy::Manual` on both arms.

### The two assertions that are not about time

- **The `flock` probe runs at most once per directory** (`flock_probe_count_for_dir`).
  This is the one cost that would scale with query volume if the support cache
  ever broke. The assertion is per directory, never on the process-global
  `flock_probe_count()`, because `cargo test` runs test binaries in parallel.
- **`ReloadPolicy::Manual` leaves no watcher threads.**
  `no_meta_file_watcher_threads_with_manual_reload` opens every index, holds the
  readers (a dropped reader takes its thread with it, which would pass for the
  wrong reason) and scans `/proc/self/task/*/comm` for
  `thread-tantivy-meta-file-watcher` — matched by its 15-byte truncated prefix,
  and matched by **name**, never by a thread count, since other tests share the
  process. The default `OnCommitWithDelay` policy spawns one such thread per
  index, each re-reading and CRC32-ing `meta.json` every 500 ms for the life of
  the process: six threads and ~12 reads/s against the user's storage volume,
  growing with every downloaded language.
