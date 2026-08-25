# Run Storage Diagnostics

**Feature PRD:** `tasks/2026-08-05-201545-prd---run-storage-diagnostics.md`
**Phase 2 (the fix):** `tasks/2026-08-05-193727-prd---fulltext-index-on-filesystems-without-flock.md`
**Background research:** `tasks/2026-08-05-193727-research---tantivy-directory-wrapper-implications.md`

## 1. Why this exists

Two users who installed the databases to an **SD card** reported that fulltext
searches return nothing, while ContainsMatch searches work and Database
Validation reports no errors. One supplied a log
(`feedback-and-bug-reports/log-chromebook.txt`) that named the cause:
`MmapDirectory::acquire_lock` calls `flock(2)` and that volume answers `ENOSYS`,
so **every** Tantivy index fails to open, the searcher holds zero indexes, and
fulltext search silently returns an empty result set.

A fix was designed — a `Directory` wrapper that falls back to a process-internal
mutex when the volume cannot do advisory locking — but it rests on an assumption
that cannot be checked from here: **does `mmap` work on that filesystem at all?**
If it does not, the wrapper is necessary but not sufficient, and phase 2 is a
much larger job. We had never observed a *successful* index read from an SD card,
because the lock failure short-circuits everything after it.

So this feature ships a build that **changes no app behaviour** and adds one
button. It measures every assumption phase 2 depends on — including running the
candidate fix end-to-end against the user's own indexes — writes the whole
report to `log.txt`, and shows it in a dialog with **Copy** and **Close**.

None of it is throwaway. `backend/src/search/lenient_directory.rs` is the real
phase-2 code, in its final location. Phase 1 wires it **only** into section E of
this report; phase 2 flips `searcher.rs` and `indexer.rs` over to it once the
data says it works.

## 2. Where it lives

| Piece | File |
|---|---|
| Report + probes | `backend/src/storage_diagnostics.rs` |
| Candidate `Directory` wrapper | `backend/src/search/lenient_directory.rs` |
| Bridge invokable + signal | `bridges/src/sutta_bridge.rs` (`run_storage_diagnostics`, `storageDiagnosticsCompleted`) |
| Results window | `assets/qml/StorageDiagnosticsDialog.qml` |
| Entry points | `assets/qml/AboutDialog.qml`, `assets/qml/DatabaseValidationDialog.qml` |
| The one instance | `assets/qml/SuttaSearchWindow.qml` |

`run_storage_diagnostics()` has **exactly one caller**: the bridge invokable
behind the button. There is deliberately no CLI subcommand and no HTTP route —
`/health` would be actively wrong for a report that writes probe files and opens
every index, and the delivery mechanism for the affected users is "please press
this button".

It reads `get_app_globals()`, which **panics** when uninitialised. The GUI
satisfies that because `gui.cpp` initialises the globals unconditionally, well
before any dialog can exist. A future headless caller must initialise them
itself rather than inherit the assumption silently.

## 3. How to read the report

The report opens with a **plain-language verdict** of two or three sentences,
then sections A–F beneath it. The verdict summarises; it never replaces.

### Verdict

`derive_verdict()` is a pure function over the collected results, so every
branch is unit-tested off-device. Two rules shape it and are easy to break by
adding a case:

- **No jargon.** `flock`, `mmap`, Tantivy, FUSE and errno names belong in the
  sections below, never in the verdict.
- **Never imply a fault that was not found.** Four measured states are *normal*
  and must never produce a fault verdict:
  1. the searcher was never initialised this session (it is opened lazily);
  2. an index the user may not have populated returns zero hits;
  3. stale `.tantivy-*.lock` files are present;
  4. `storage_path_state()` reports `absent` on **desktop** — which it always
     does, because a recorded storage path is a mobile-only concept.

When the results match no known pattern the verdict says so and asks for the
summary. An unrecognised pattern is exactly the case we most want reported —
inventing a diagnosis would lose it.

### Section A — Storage location

The **recorded** path (what the user chose, from `storage-path.txt`) and the
**resolved** path (what the app is actually using, which silently falls back to
the internal app root), with an explicit note when they differ. Conflating those
two was the original bug in the relocated-storage feature — see
[relocated-storage-recovery.md](./relocated-storage-recovery.md).

Then the `storage_path_state()` verdict, free/total space, the filesystem type
and mount options from `/proc/mounts`, the `statfs` magic, and whether the
resolved path is internal or on an external volume.

The mount line and the `statfs` magic are **both** printed when both are
available: they disagree on sdcardfs/FUSE stacks, and the disagreement is itself
informative.

### Section B — Primitive probes

The four measurements that decide phase 2, each with its elapsed time (SD cards
are slow, and phase 2 needs to know whether the wrapper is viable on latency
grounds):

| Probe | What it answers |
|---|---|
| `flock` | `supported` / `busy` / `unsupported(<errno> <name>)` / `error(<errno>)`. `Ok(false)` is **busy**, not unsupported. |
| `mmap` | **The go/no-go.** Maps a real index segment file read-only and reads three bytes: first, middle, last. |
| atomic write | Write a temp file, `sync_data()`, rename it over an **existing** target — exactly what `MmapDirectory::atomic_write` does. The index *write* path. |
| plain read/write | Create, write, `fsync`, re-read, delete. Separates "the volume is broken" from "the volume is fine but lacks one primitive". |

Two details of the `mmap` probe are load-bearing. The **middle and last reads**
force page faults beyond page 0, which is where a `direct_io` FUSE mount fails —
reading only byte 0 measures nothing. And the file is chosen **by size, not by
extension**: index directories hold 146-byte `.fast` files next to multi-MB
`.store` files, and an extension allowlist can select a file too small to fault
past page 0. If nothing clears the 8 KiB floor, the probe writes its own 64 KiB
file and says so.

`EOPNOTSUPP` and `ENOTSUP` are the **same number (95)** on Linux and Android, so
the errno→name lookup does not pretend to tell them apart. On Windows,
`raw_os_error()` yields Win32 codes rather than CRT errnos, so the
classification is per-platform — comparing a `LockFileEx` failure against libc's
Windows constants would misclassify ordinary errors as "this filesystem does not
do advisory locking", and that verdict is *cached* for the process lifetime.

**Which directory gets probed.** Ideally a per-language index directory, since
that is where the failure happens. When there is none — an install whose index
download never finished has none, and that user is exactly who presses this
button — the probes fall outwards to the index root, then to the storage root,
and the report names which one it used (`ProbeDirSource`). Skipping section B
there would throw away the `mmap` reading, which is the whole point of the
exercise.

### Section C — Index inventory

Per index directory under `suttas/`, `dict_words/` and `library/`: language key,
file count, total size, and whether `meta.json` is present and parseable. Plus
the index `VERSION`, reported **once** as a section header — it is one file at
the top of the index tree (`<app-assets>/index/VERSION`), not per-language — and
whether it matches `INDEX_VERSION`.

Stale `.tantivy-meta.lock` / `.tantivy-writer.lock` files are reported with
their age. **Their presence is informative, not alarming:** `MmapDirectory`
creates them and never deletes them (`ReleaseLockFile`'s `Drop` only closes the
fd), so any successful open leaves them behind.

### Section D — Index open, as the app does it today

The **current** three-step sequence per directory —
`MmapDirectory::open` → `Index::open` → `index.reader()` — reporting **exactly
which step fails**, with the full error. The existing log conflates all three
behind one message (`searcher.rs:121`); this does not.

`Index::open` takes no lock and is expected to succeed; `index.reader()` is the
step that fails on a volume without `flock`.

Note that the app opens indexes with `open_or_create`, which brings an empty
index into being when the files are missing; the diagnostic only ever uses
`Index::open`. So a missing or incomplete index fails at step 2 here where the
app would instead carry on and simply find nothing. The report says so inline.

### Section E — Index open through the candidate fix

The same three steps through `LenientLockMmapDirectory`, with
`ReloadPolicy::Manual`, followed by two real searches. **This section is the
point of the whole exercise:** a non-zero hit count from a user's SD card is
direct proof the phase-2 fix works on real hardware, obtained before we commit
to it.

Per directory it reports:

- **Which lock route the wrapper took** — and *every distinct* route, not just
  the last one. There are four: the inner `flock` succeeded; it fell back after
  an unsupported-operation errno; it fell back after **some other `IoError`**;
  or the lock was genuinely busy and that was propagated rather than masked.
  The third matters because `MmapDirectory::acquire_lock` *opens* the lock file
  before locking it, so an unwritable directory also yields `LockError::IoError`
  — and the fallback would turn that into a clean-looking success on a genuinely
  broken volume. Without the distinction, section E cannot tell "the fix works"
  from "the fix hid the failure". One `index.reader()` reaches `acquire_lock`
  more than once (`open_segment_readers()` takes `META_LOCK` on every reader
  build), so a single overwritten slot could hide an early `IoError` behind a
  later success — which is precisely the pair being distinguished.
- **`num_docs`** — nothing else in the report measures it (C counts *files*, D
  and E measure *opens*), yet the expected-vs-unexpected split for a zero hit
  count depends on it entirely. It is also independent proof that the read path
  works even if both query terms come up unlucky.
- **Both query terms against every index**, as `nirodha=N cessation=M`. The terms
  are hard-coded on purpose: reproducible across every report, and one less thing
  to explain to a user who is already confused. Routing them by language would
  send an English term at `suttas/san` (romanized Sanskrit), so a perfectly
  healthy populated index would score zero and read as a fault.

A **zero hit count is expected, not a fault**, from an index holding no
documents — the library index above all, since most users import no books, but
equally a language whose content was never downloaded. The informative case is a
zero hit from an index that *does* contain documents, and the report labels the
two differently. A search that **fails outright** is a third case again, and is
labelled as such.

The reader is built with `ReloadPolicy::Manual` here, but that is **not** what
makes the difference: the reader takes the same lock on every build whatever
that setting is, so a success in section E is attributable to the wrapper alone.
The report says this inline so nobody has to take it on trust.

Section E registers tokenizers (section D deliberately does not): the
`QueryParser` resolves `{lang}_stem` / `{lang}_normalize` off the `Index` at
parse time. Because `Index::open` reads the schema from `meta.json` rather than
being handed one, `schema().get_field("content")` can legitimately fail on a
foreign, truncated or older `meta.json` — that is reported as its own attributed
line, not an aborted section.

### Section F — Live searcher state

What the process-global `FULLTEXT_SEARCHER` currently holds (sutta / dict /
library index counts) and the per-directory open failures recorded at startup,
plus app version, platform and Android API level.

**"Searcher not initialised this session" is a state of its own**, distinct from
"0 indexes, 0 failures": `init_fulltext_searcher()` is lazy and mode-gated, and
the diagnostics are forbidden from initialising it to find out. That state is
derived from `with_fulltext_searcher()` returning `None` — **not** from
`is_fulltext_searcher_ready()`, which returns `true` whenever the global is
`Some` regardless of index count. That dishonesty is real, but fixing it is
phase-2 work: it feeds `/health`'s `fulltext_searcher_ready` field, and changing
it here would break the "no behaviour change" guarantee.

The failure list is cleared by **both** `FulltextSearcher` constructors
(`open()` and `open_from_dirs()`, via the shared `begin_open_session()` helper),
so an entry recorded before a storage recovery is never reported as a live
fault.

## 4. Decision gate — what each outcome means for phase 2

| Section B `flock` | Section B `mmap` | Section E hits | Conclusion |
|---|---|---|---|
| unsupported | ok | > 0 | **Diagnosis and fix both confirmed.** Proceed with phase 2 as written. |
| unsupported | ok | 0 or error | Lock diagnosis right, wrapper wrong. Redesign phase 2 §4.1 from the reported error. |
| unsupported | **fails** | 0 | **Wrapper is necessary but not sufficient.** Phase 2 needs a non-mmap `Directory` (pread-backed or read-into-RAM `FileHandle`) — a much larger job, correctly scoped *before* it starts. |
| supported | ok | > 0 | The reporting user's fault is **something else**; re-triage from sections A, C, D. |

**It has been run for real, and it landed on row 1.** A ChromeOS user (151.0.7922.168,
Android API 33, storage on a `fuse` external volume) ran it on 2026-08-25:
`flock` **unsupported(38 ENOSYS)**, `mmap` **ok** on an 18 MB segment file read
at offsets 0 / middle / last, and section E opened all six indexes through
`LenientLockMmapDirectory` and returned real hits (`dict_words/pli`: 539,569
docs, `nirodha=2227` in 328.6 ms) where section D and the live searcher opened
**zero**. Phase 2 is unblocked and needs no redesign; **the non-mmap `Directory`
contingency is not needed.** Full report:
`tasks/2026-08-05-201545-prd---run-storage-diagnostics.md` §12, raw material in
`feedback-and-bug-reports/rechromebookstoragetesting/`.

Two things that run counter to how the feature was framed, worth knowing before
you read the next report: the affected volume is a **ChromeOS/ARCVM FUSE mount,
not an SD card**, and it is **fast** (reader builds 77–628 ms, searches
0.1–329 ms). Nothing measured supports a "searches will be slower here" message.

## 5. Safety constraints

These are requirements, not incidental properties, and each has a test:

- **Read-only with respect to app data.** The only writes are the diagnostic's
  own probe files, all named `simsapa-*`.
- **Every probe file is removed on every exit path** — success, failure and
  panic — via a `Drop` guard (`DiagCleanup`, and `ProbeCleanup` inside
  `probe_flock_support`). The probes return early from a dozen places, so the
  guarantee comes from `Drop`, never from any one exit path being tidy. Leaving
  litter on a user's card is a user-visible defect.
- **Never create or modify an index.** All opens use `Index::open`, never
  `Index::open_or_create`.
- **Never disturb the live searcher.** The diagnostic opens its own `Directory`
  and `Index` instances and does not touch, replace or reinitialise
  `FULLTEXT_SEARCHER`.
- **No personal or sensitive data.** No sutta or dictionary content, no API keys,
  no bookmark or history data. Volume UUIDs and absolute paths are expected and
  necessary.
- **`try_exists()`, never `.exists()`** — the Android rule in CLAUDE.md.
- **A failure in any one probe must not abort the run.** Each section catches its
  own errors, prints them, and continues. A diagnostic that crashes on the broken
  case is useless.
- **No leaked threads.** Section D reproduces the app's bare `index.reader()`,
  i.e. the default `ReloadPolicy::OnCommitWithDelay`, which spawns a 500 ms
  `meta.json`-polling thread per index for the reader's lifetime. Each reader is
  dropped as soon as its steps are recorded.

### The one unavoidable write to app data

Section D's `index.reader()` reaches `MmapDirectory::acquire_lock`, which
**opens (and therefore creates) `.tantivy-meta.lock` before locking it**. On an
index directory that has never been opened successfully, section D thus leaves
behind lock files that were not there before, and the cleanup guards do not
remove them — they are scoped to the diagnostic's own `simsapa-*` names.

This is accepted rather than prevented: the files are zero-length, Tantivy never
deletes them anyway, and any ordinary app startup creates them. Two things
follow, and both are implemented:

1. Section C's lock-file reading is **collected before section D executes**, so
   the diagnostic does not report its own leftovers as the volume's prior state.
   The section ordering in the *report* already matched; the requirement is that
   the **execution** order match too.
2. Where section C recorded a lock file as absent and the run created one, the
   report says so in a single line, so a maintainer is not misled about the
   volume's prior state.

## 6. The UI

One `StorageDiagnosticsDialog` instance exists, declared in
`SuttaSearchWindow.qml`; both entry points call `open_and_run()` on it.

Three things about it are load-bearing and easy to break:

- **`modality: Qt.ApplicationModal`.** `DatabaseValidationDialog` is itself
  `ApplicationModal`, and a **non-modal** window opened from it is input-blocked
  for as long as that modal stays open — it would appear and be dead to clicks.
  A modal shown *later* heads the modal stack and is not itself blocked, which is
  why matching the modality is the fix rather than removing it from Database
  Validation. `AboutDialog` sets no modality, so the About entry point does not
  expose the bug: **test the Database Validation path.**
- **`required property int extra_top_margin`**, bound from `SuttaSearchWindow`
  like every other dialog sibling. An unbound `required property` is a *runtime*
  QML error that a successful `make build -B` will not reveal.
- **The window owns the whole run** — `open_and_run()`, the `Connections` on the
  process-global `storageDiagnosticsCompleted`, the busy state, the
  "initiated here" guard and its own `AssetManager` for the keep-screen-on
  bracket. The two entry-point dialogs only call `open_and_run()`. Splitting the
  guard across them would put three objects on one global signal with the flag in
  the wrong two.

It follows the `ApplicationWindow` rules of
[android-edge-to-edge-and-safe-areas.md](./android-edge-to-edge-and-safe-areas.md):
root content anchored to its parent, never sized from `root.width`/`root.height`,
and no `topPadding`/`padding` assigned on the root.

The run happens on a background thread; a panic in the worker is caught so the
completion signal still fires rather than leaving the UI on a busy indicator
forever. The keep-screen-on lock is released in the completion handler on
**both** success and failure — never in `onClosed`, since the backend job
continues after the dialog closes.

The complete summary is also written to `log.txt` at INFO in **one** call, so it
lands contiguously and a user who sends only their log has still given us
everything.

## 7. Verifying a change to this feature

```sh
cd backend && cargo test --lib storage_diagnostics
cd backend && cargo test --lib lenient          # the wrapper's own tests
make qml-test
make build -B
```

The unit tests cover the mount-table parser against Android sdcardfs/FUSE and
plain Linux fixtures, every verdict branch (including the four not-a-fault
states), the probe-cleanup guarantee, the fallback probe-directory choice, the
report builder end-to-end, and — as regressions — that the diagnostic creates no
index, leaves no watcher threads, and keeps no copy of the wrapper's lock logic.

The two entry points must be checked **separately on a real build**, Database
Validation first, for the two defects above that no test and no green build can
show.
