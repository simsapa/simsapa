# Research — technical implications of the `Directory` wrapper approach

**Companion to:** `2026-08-05-193727-prd---fulltext-index-on-filesystems-without-flock.md`
**Date:** 2026-08-05
**Basis:** direct reading of `tantivy-0.25.0` sources in
`~/.cargo/registry/src/index.crates.io-*/tantivy-0.25.0/` plus the app's call
sites. Every claim below cites a file:line.

**Verdict: the wrapper is viable and is the right approach.** Five findings
change the PRD; one of them (mmap on FUSE) is a genuine go/no-go risk that
source reading cannot settle, and one (`FR-11`) is a factual correction to the
PRD as written.

---

## 1. The wrap is safe: nothing downcasts `Directory`

Grepped the whole crate for `downcast`, `is::<`, `as_any`, `MmapDirectory>`:
the only downcasting in tantivy is on `Fruit` (collectors) and `Scorer`
(queries). **No code path ever recovers the concrete `MmapDirectory` from a
`Box<dyn Directory>`.** A wrapper is therefore fully transparent — there is no
hidden fast path we would lose.

The trait surface is small. `MmapDirectory`'s `impl Directory`
(`directory/mmap_directory.rs`) defines exactly nine methods:

`get_file_handle`, `delete`, `exists`, `open_write`, `atomic_read`,
`atomic_write`, `acquire_lock`, `watch`, `sync_directory`.

Notably it does **not** override `open_read`, which has a default
implementation that calls `get_file_handle` (`directory/directory.rs:121`). So
delegating the nine gives correct behaviour for the whole trait.

`DirectoryClone` (`directory.rs:229`) has a blanket impl for
`T: Directory + Clone`, and `MmapDirectory` is `Clone` (it is an `Arc` inside),
so the wrapper only needs `#[derive(Clone)]`.

**Maintenance implication:** all nine methods are *required* (no defaults), so a
future tantivy version adding a required method breaks the build loudly rather
than silently mis-delegating. That is the failure mode we want.

## 2. `DirectoryLock` is `Send + Sync` — this rules out `MutexGuard`

```rust
// directory/directory.rs:43,50
pub struct DirectoryLock(Box<dyn Send + Sync + 'static>);
impl<T: Send + Sync + 'static> From<Box<T>> for DirectoryLock { … }
```

`std::sync::MutexGuard` is **`!Send`**. `parking_lot::MutexGuard` (0.12.3, already
a backend dependency) is also `!Send` unless the crate-wide `send_guard` feature
is enabled — and that feature is global to the dependency graph, so turning it on
to satisfy one call site is the wrong trade.

**Therefore the fallback guard must be hand-rolled**: a small
`struct LenientLockGuard { slot: Arc<LockSlot> }` where `LockSlot` is e.g.
`(Mutex<bool>, Condvar)`, taking/releasing the flag in `lock()`/`Drop`. Such a
struct is plainly `Send + Sync` and boxes into `DirectoryLock` without trouble.
This is a concrete constraint the implementer would otherwise discover only at
the compiler error.

## 3. The fallback must be a real mutex — a no-op would be unsafe

It is tempting to make `acquire_lock` return a dummy guard when `flock` is
unsupported. **That would be a correctness bug**, and the app's own structure is
what makes it one.

`META_LOCK` exists to stop the garbage collector deleting segment files while a
reader is opening them. Both sides are live in Simsapa simultaneously:

- **Reader side:** `InnerIndexReader::open_segment_readers`
  (`reader/mod.rs:194`) takes `META_LOCK` around `searchable_segments()` +
  `SegmentReader::open`.
- **GC side:** `ManagedDirectory::garbage_collect` (`managed_directory.rs:138`)
  takes `META_LOCK` around the living-files computation.
- **GC runs automatically**, not just on demand: `segment_updater.rs:451` (after
  commit) and `:674` (after merge), on the writer's thread pool.

And in this app the old searcher stays alive across a rebuild: every mutation
site calls `reinit_fulltext_searcher()` **after** the write completes
(`backend/src/lib.rs:398`; `bridges/src/sutta_bridge.rs:3826`, `:3925`, `:4047`),
so `FULLTEXT_SEARCHER` still holds open readers on the very index a rebuild or a
dictionary import is committing and merging into.

So reader-open genuinely races GC-delete inside one process, and the
process-internal mutex is load-bearing. PRD FR-4 and FR-6 are correct as written;
the alternative "just return a dummy lock" must be explicitly rejected.

## 4. Correction to the PRD: `Index::open_or_create` takes **no** lock

PRD **FR-11** implies switching `open_or_create` → `open` is part of the fix. It
is not. Verified:

- `Index::open` (`index/index.rs:510`) = `ManagedDirectory::wrap` +
  `load_metas`. **No `acquire_lock`.**
- `IndexBuilder::open_or_create` (`index/index.rs:218`) = `Index::exists` (a
  plain `exists()` on `meta.json`) then `Index::open`, or `create`. **No
  `acquire_lock` on either branch.**

There are exactly **two** lock sites in the whole crate:

| Site | Lock | `is_blocking` | Failure surfaces as |
|---|---|---|---|
| `reader/mod.rs:194` — reader open/reload | `META_LOCK` | **true** | `LockError::IoError(ENOSYS)` — *the one in the log* |
| `index/index.rs:545` — `Index::writer()` | `INDEX_WRITER_LOCK` | false | **`LockError::LockBusy`** — real errno discarded by `try_lock_exclusive().map_err(\|_\| LockBusy)` (`mmap_directory.rs:487`) |

The Chromebook log independently confirms this: `list_indexed_source_uids` logs a
distinct message for the `Index::open_or_create` step
(`indexer.rs:824`, *"open index …"*) and for the reader step (`indexer.rs:833`,
*"reader …"*). **Only the `reader` message ever appears.** So
`Index::open_or_create` *succeeded* on the SD card — the directory is readable
and writable, and `flock` is the sole failure.

FR-11 remains worth doing as hygiene (the search path should not be able to
create an index), but the PRD must not describe it as part of the fix.

## 5. Blocking semantics must be preserved per-lock

The fallback cannot be one-size-fits-all:

- `META_LOCK` is `is_blocking: true` → the fallback must **block** (`lock()`).
- `INDEX_WRITER_LOCK` is `is_blocking: false` → the fallback must **`try_lock()`
  and return `LockBusy`** on contention. Doing otherwise would silently permit
  two `IndexWriter`s on one index in-process, which is exactly the corruption
  tantivy's non-blocking lock exists to prevent.

## 6. Re-entrancy: safe today, fragile against upgrades

A non-reentrant mutex deadlocks where `flock` merely blocks-then-succeeds. Two
places were checked:

- `ManagedDirectory::garbage_collect` scopes `META_LOCK` inside an inner block
  that closes **before** the delete loop, with an explicit upstream comment:
  *"releasing the lock as `.delete()` will use it too"* (`managed_directory.rs`,
  above line 138).
- `ManagedDirectory::delete` just delegates; it takes no lock.

So there is no nesting today. But this is an upstream implementation detail we
would be depending on. **Recommendation:** make the blocking fallback a
`try_lock` in a bounded retry loop (mirroring tantivy's own
`RetryPolicy { num_retries: 100, wait_in_ms: 100 }` at `directory.rs:89`) rather
than an unbounded `lock()`, and log loudly on exhaustion. A future tantivy that
nests the lock then produces a logged error instead of a frozen UI thread.

## 7. Lock-file litter on the user's card

`MmapDirectory::acquire_lock` **creates** `.tantivy-meta.lock` /
`.tantivy-writer.lock` and never removes them — `ReleaseLockFile`'s `Drop`
(`mmap_directory.rs:305`) only closes the fd. They are also excluded from GC,
since `is_managed()` skips dotfiles. So the app already leaves these files on
users' SD cards today.

**Implication for the design:** once the probe (FR-7/FR-8) has returned
*unsupported* for a directory, the wrapper should go **straight to the mutex**
and not call the inner `acquire_lock` at all. That saves an `open()` + failing
`flock()` syscall on every reader reload and avoids creating a lock file that can
never serve its purpose on that volume.

## 8. NEW — the app spawns a polling thread per index, at 2 Hz, forever

Unrelated to the bug but found while tracing `index.reader()`, and it matters
more on the SD-card/mobile configuration than anywhere else.

`FulltextSearcher::open_single_index` (`backend/src/search/searcher.rs:140`)
calls a bare `index.reader()`, which defaults to
`ReloadPolicy::OnCommitWithDelay` (`reader/mod.rs:52`). That branch calls
`directory().watch(…)` (`reader/mod.rs:203`), and `MmapDirectory`'s watcher is
**not inotify — it polls**:

```rust
// directory/file_watcher.rs:12,47-62
const POLLING_INTERVAL: Duration = Duration::from_millis(500);
// … a dedicated thread that opens meta.json, reads it line by line,
//    CRC32s it, compares, sleeps 500 ms, repeats — for the life of the process.
```

With the six indexes in the log (`suttas/{pli,en,san}`, `dict_words/{pli,en}`,
`library/en`) that is **six threads performing ~12 file reads per second against
the SD card, indefinitely** — and it grows with every sutta language the user
downloads.

The app does not need it: every index mutation is already followed by an explicit
`reinit_fulltext_searcher()` (see §3 for the call sites), which rebuilds the
readers wholesale.

**Recommendation:** set `.reload_policy(ReloadPolicy::Manual)` on the search-path
reader builder. Battery and I/O win on every platform, most of all on mobile.

**This is not an alternative fix** — `Manual` skips the *watcher*, but
`InnerIndexReader::new` → `create_searcher` → `open_segment_readers` takes
`META_LOCK` unconditionally regardless of reload policy. The wrapper is still
required.

## 9. The real open risk: does `mmap` work on this FUSE mount?

This is the one question source reading cannot answer, and it decides whether the
wrapper is sufficient or merely necessary.

`MmapDirectory::get_file_handle` → `open_mmap` → `memmap2::Mmap::map(&file)`
(`mmap_directory.rs`). Every index read goes through a `MAP_SHARED` read-only
mapping.

Why the existing evidence proves nothing either way:

- **SQLite working is not evidence.** Android's SQLite defaults to
  `PRAGMA mmap_size = 0` and uses `pread`/`pwrite`. The databases working on the
  card says nothing about `mmap`.
- **We have never observed a successful index read from an SD card**, because
  `acquire_lock` fails first and the failure short-circuits everything after it.
- FUSE filesystems mounted with `direct_io` reject `mmap(MAP_SHARED)` with
  `ENODEV`. Whether Android's FuseDaemon does this for `/storage/<UUID>/Android/data/…`
  on a portable volume is not something we can determine from here.

If `mmap` fails, the wrapper unblocks the open and searches then fail with a
*different* error, and the real fix becomes a non-mmap `Directory`
(`pread`-backed or read-into-RAM `FileHandle`) — a substantially larger job.

**Recommendation (cheap, high value):** extend the FR-8 probe so it *also*
mmaps a small file in the index directory and records the verdict, and log both
verdicts at startup (FR-26). This costs a few lines, converts the unknown into
data we collect from the next affected user's log, and can be done **before**
committing to the wrapper implementation. It is the single highest-value item to
sequence first.

## 10. Lower-risk odds and ends

- **`atomic_write` on exFAT** (`mmap_directory.rs:352`): `tempfile_in(parent)` →
  `write_all` → `flush` → `sync_data()` → `persist()` (i.e. `rename(2)`).
  Same-directory rename-over-existing is supported on exFAT via FUSE. Low risk,
  and only on the write path.
- **`flock` is via `fs4`**, not std: `use fs4::fs_std::FileExt`
  (`mmap_directory.rs:10`), which is `flock(2)` on Unix. Confirms the diagnosis.
- **Probe subtlety:** `try_lock_exclusive()` returning `Ok(false)` means *busy*,
  not *unsupported* — only an `Err` with `ENOSYS`/`EOPNOTSUPP`/`ENOTSUP`/`EINVAL`
  means unsupported. On a filesystem where `flock` silently no-ops and always
  succeeds (some network filesystems), the probe reports *supported* and we keep
  the real path — harmless for a single-process app.
- **Performance:** the wrapper adds one cached branch per `acquire_lock` call —
  once per reader open/reload and once per writer construction. Immeasurable.

---

## Recommended changes to the PRD

1. **Correct FR-11** — `Index::open_or_create` takes no lock; the switch to
   `Index::open` is hygiene, not part of the fix. Correct the §7 lock table
   accordingly.
2. **Add a requirement** that the fallback guard be a hand-rolled `Send + Sync`
   type, with the reason (§2), and that a no-op lock is explicitly rejected (§3).
3. **Add a requirement** that blocking/non-blocking semantics are preserved
   per-lock (§5), and that the blocking fallback use a bounded retry rather than
   an unbounded block (§6).
4. **Add a requirement** that once a directory is known unsupported, the inner
   `acquire_lock` is skipped entirely (§7).
5. **Add a requirement** for `ReloadPolicy::Manual` on the search path (§8),
   noting it is an independent improvement and not a fix.
6. **Promote the mmap question from Open Question to a sequenced FR** — probe
   `mmap` alongside `flock` and land that probe *first* (§9).
