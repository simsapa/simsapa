# Fulltext index storage and file locking

The fulltext (Tantivy) index has to open on storage that does not implement
`flock(2)`. ChromeOS/ARCVM's `fuse`-backed external volumes and portable SD
cards are two instances of that one class — the issue is the **filesystem's
locking support**, not the hardware. `backend/src/search/lenient_directory.rs`
(`LenientLockMmapDirectory`) is the wrapper that makes those volumes work, and
it is wired into every real search and index path.

> **This document is partial.** The mechanism sections (the `flock`-vs-`fcntl`
> distinction, the two Tantivy lock sites and their differing failure mappings,
> the single-process invariant the fallback depends on, and the `mmap`
> measurement that let the non-mmap contingency be dropped) are written as part
> of task 8.2 of
> `tasks/2026-08-25-190522-tasks-fulltext-fix-and-dictionary-import-overhaul.md`.
> The benchmark below is complete and is the record for task 3.7.

## Benchmark — the wrapper costs nothing on a normal filesystem

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
