# PRD — "Run Storage Diagnostics"

**Date:** 2026-08-05
**Status:** Draft — not yet implemented
**Phase:** 1 of 2. The fix itself is
`2026-08-05-193727-prd---fulltext-index-on-filesystems-without-flock.md`
(phase 2), which is **blocked on the data this PRD collects**.
**Background research:**
`2026-08-05-193727-research---tantivy-directory-wrapper-implications.md`

## 1. Introduction / Overview

Two users who installed the databases to an **SD card** report that fulltext
searches return no results, while ContainsMatch searches work and Database
Validation reports no errors. One of them supplied a log
(`feedback-and-bug-reports/log-chromebook.txt`) that identifies the cause:
Tantivy's index directories cannot be opened because `flock(2)` returns
`ENOSYS` on that volume, so the app opens **zero** indexes and silently returns
empty results.

A fix has been designed (phase 2), but it rests on assumptions we **cannot
verify from here** — chiefly whether `mmap` works on that filesystem at all,
which decides whether the designed fix is sufficient or merely necessary. We
have never observed a successful index read from an SD card, because the lock
failure short-circuits everything after it.

**This PRD delivers a build that changes no app behaviour**, but adds a
**"Run Storage Diagnostics"** action. It measures every assumption phase 2
depends on — including running the candidate fix end-to-end — writes everything
to the log, and presents a summary dialog with **Copy** and **Close** so the
user can send us the summary text along with their `log.txt`.

Crucially, this is **not throwaway work**: the probes and the candidate
`Directory` wrapper written here are the same code phase 2 ships. Phase 1 wires
them only into the diagnostic; phase 2 flips them into the real search path once
the data says they work.

## 2. Goals

1. Answer, from a real affected device, the three questions that gate phase 2:
   - Is `flock` genuinely unsupported on this volume, and with which errno?
   - **Does `mmap` work on this volume?** (the go/no-go)
   - Does the candidate wrapper actually open the indexes and return real search
     hits there?
2. Give the user a single button that produces one block of text they can copy
   and send, with no technical steps and no file hunting.
3. Write the same information to `log.txt`, so the log alone is sufficient if the
   user sends only that.
4. Change **no** existing app behaviour: same search results, same startup, same
   storage selection, on every platform.
5. Leave the volume exactly as found — no probe files, no index modification.
6. Produce the reusable probe + wrapper code that phase 2 will adopt unchanged.

## 3. User Stories

- **As a user whose searches return nothing**, I press one button, get a block of
  text, press **Copy**, and paste it into an email — without being asked to find
  files or run commands.
- **As a maintainer**, I receive a summary that tells me the filesystem type, the
  `flock` and `mmap` verdicts, exactly which step of the Tantivy open fails, and
  **whether the proposed fix would have worked** — from the user's own device,
  before I write the fix into the product.
- **As a maintainer**, if `mmap` turns out to be broken too, I learn it now
  rather than after shipping a fix that does not fix it.
- **As a user on a perfectly healthy desktop install**, I never notice this
  feature exists, and nothing about my app behaves differently.

## 4. Functional Requirements

### 4.1 Entry point

1. A **"Run Storage Diagnostics"** button must be added to the button row at the
   bottom of `assets/qml/AboutDialog.qml`, alongside the existing "Copy App Info"
   and "Close" buttons. This is the right home because that dialog already holds
   the log-file list with per-file **Save** and **Copy** actions
   (`AboutDialog.qml:181-241`), so it is where users are already directed when we
   ask for logs.
2. The same action must also be reachable from
   `assets/qml/DatabaseValidationDialog.qml`, since a user investigating "no
   search results" is likely to open that first. Both entry points must open the
   **same single** results-dialog instance (see FR-10), not one each.
3. The button must be available on **all platforms**, not just mobile — the same
   diagnosis applies to a desktop user with an external or network drive.
4. The diagnostics run must happen **off the UI thread**, with a busy indicator
   and the button disabled while it runs. The run touches an SD card and opens
   Tantivy indexes; it may take several seconds.
5. The run must be bracketed by `AssetManager.set_keep_screen_on(true)` /
   `(false)`, released on **both** success and failure paths, per CLAUDE.md.

### 4.2 The results dialog

6. On completion a dialog must show the **summary text** in a monospace,
   **selectable**, scrollable text area.
7. The dialog must have a **"Copy"** button that copies the entire summary to the
   clipboard, reusing the existing invisible-`TextEdit` `clipboard_helper`
   pattern (`AboutDialog.qml:63-71`).
8. The dialog must have a **"Close"** button.
9. The dialog must display a short instruction line telling the user to send both
   the copied text **and** their `log.txt`, and stating that `log.txt` can be
   saved or copied from the log-file list in the About dialog.
10. The dialog's root must be an **`ApplicationWindow`**, matching its two entry
    points (`AboutDialog.qml` and `DatabaseValidationDialog.qml` are both
    `ApplicationWindow` roots, declared as inline siblings of
    `SuttaSearchWindow.qml` at `:2257` and `:2277`), and there must be **one**
    instance, declared in `SuttaSearchWindow.qml` and opened by both entry
    points. It must follow the `ApplicationWindow` rules of
    `docs/android-edge-to-edge-and-safe-areas.md`: root content anchored to its
    parent, never sized from `root.width`/`root.height`, and **no
    `topPadding`/`padding` assigned on the root** (which would silently replace
    Qt's safe-area binding). The `Popup`-family padding rule does not apply.
    Sizing the *window* is expected — `DatabaseValidationDialog.qml:12-13` is the
    shape to copy. Per `docs/startup-sequence-and-caches.md`, an
    `ApplicationWindow` root cannot be wrapped in a `Loader`; deferring it
    requires `Component` + `createObject`.
    - **FR-10a — modality.** The results window must be
      **`modality: Qt.ApplicationModal`**. `DatabaseValidationDialog.qml:18` is
      `Qt.ApplicationModal`; a **non-modal** window opened from it is input-blocked
      for as long as that modal stays open, so the results dialog would appear and
      be unclickable. `AboutDialog.qml` sets no modality, so the About entry point
      (the one implemented and tested first) does **not** expose the bug. A modal
      window shown *later* heads the modal stack and is not itself blocked, which
      is why matching the modality is the fix rather than removing it from
      Database Validation.
    - **FR-10b — `extra_top_margin`.** Every dialog sibling in
      `SuttaSearchWindow.qml` declares `required property int extra_top_margin`
      (`AboutDialog.qml:29`, `DatabaseValidationDialog.qml:25`) and is bound with
      `extra_top_margin: root.extra_top_margin` (`SuttaSearchWindow.qml:2257-2295`).
      The new window must follow suit. An unbound `required property` is a
      **runtime** QML error that a successful `make build -B` will not reveal.
    - **FR-10c — one owner for the run.** The results window owns the whole
      operation: the `open_and_run()` entry, the `Connections` on
      `storageDiagnosticsCompleted`, the busy state, the "initiated here" guard
      and its own `AssetManager` for the FR-5 bracket. The two entry-point
      dialogs only call `open_and_run()`. Splitting the guard across the entry
      points would put three objects on one process-global signal with the flag in
      the wrong two.
11. The complete summary must **also** be written to the log via the Rust logger
    at INFO, so a user who sends only `log.txt` has given us everything.

### 4.3 What the diagnostics must measure

The summary must be plain text, sectioned with headers, safe to paste into an
email. Each section reports its own elapsed time.

**Section A — Storage location**

12. The **recorded** storage path (from `storage-path.txt`) and the **resolved**
    path, and an explicit note when they differ (see
    `docs/relocated-storage-recovery.md` — conflating these was the original
    bug in that feature).
13. The `storage_path_state()` verdict
    (`absent` / `unreachable` / `reachable_empty` / `ok`). **On desktop this is
    always `absent`**: `storage_path_state()` short-circuits on `!is_mobile()`
    (`backend/src/lib.rs:1080-1082`), because a recorded storage path is a
    mobile-only concept. The section must say so in words rather than printing a
    bare `absent`, and must flag the case so the FR-34 verdict does not report
    "the storage location is unreachable" on every healthy desktop install
    (FR-37).
14. Free and total space on the volume. `fs4` — already required by the FR-17
    probe — provides this cross-platform via `fs4::statvfs` /
    `available_space` / `total_space` (rustix on unix and Android,
    `GetDiskFreeSpaceExW` on Windows), so no per-platform helper and no `libc`
    dependency is needed for it.
15. The **filesystem type and mount options**, obtained by parsing `/proc/mounts`
    for the longest mount point that prefixes the resolved path, plus the
    `statfs` `f_type` magic as a numeric fallback. This is what distinguishes
    FUSE / sdcardfs / exFAT / ext4 / F2FS, and is the single most useful line for
    triage.
16. Whether the path is on internal storage or an external volume.

**Section B — Primitive probes**

17. **`flock` probe.** Create (or open) a probe file in the index directory,
    attempt `try_lock_exclusive()`, and classify the outcome as
    `supported` / `busy` / `unsupported(<errno> <name>)` / `error(<errno>)`.
    `ENOSYS`, `EOPNOTSUPP`, `ENOTSUP` and `EINVAL` mean unsupported; `Ok(false)`
    means busy, **not** unsupported. The raw errno must be printed. Note that
    `EOPNOTSUPP` and `ENOTSUP` are the **same number (95)** on Linux and Android,
    so the errno→name lookup must not pretend to distinguish them. `fs4 0.13` is
    rustix-based and exports no errno constants, so `libc` is needed for these —
    and for nothing else.
18. **`mmap` probe — the go/no-go measurement.** Memory-map an **existing index
    segment file** read-only and read three bytes: the first, one from the middle,
    and the last. The middle and last reads matter: they force page faults beyond
    page 0, which is where a `direct_io` FUSE mount fails. Report success or the
    exact errno. The file must be chosen **by size, not by extension** — the
    largest regular file of at least ~8 KiB, skipping `meta.json`,
    `.managed.json`, `VERSION` and `*.lock`. An extension allowlist can select a
    200-byte file, which cannot fault past page 0 and so measures nothing. If no
    file meets the size floor, mmap the diagnostic's own probe file written large
    enough to span several pages, and say so in the output.
19. **Atomic-write probe.** Write a temp file in the directory, `sync_data()`, and
    `persist()` (rename) it over an existing target, then delete it. This is
    exactly what `MmapDirectory::atomic_write` does
    (`mmap_directory.rs:352`) and covers the index **write** path.
20. **Plain read/write probe.** Create, write, `fsync`, re-read and delete a small
    file — to separate "the volume is broken" from "the volume is fine but lacks
    a specific primitive".
21. Every probe must report its elapsed time. SD cards are slow, and phase 2 needs
    to know whether the wrapper is viable on latency grounds.

**Section C — Index inventory**

22. For each per-language index directory under `suttas/`, `dict_words/` and
    `library/`: the language key, file count, total size, and whether `meta.json`
    is present and parseable.
23. The presence and age of any stale `.tantivy-meta.lock` / `.tantivy-writer.lock`
    files. (`MmapDirectory` creates these and never deletes them —
    `ReleaseLockFile`'s `Drop` only closes the fd — so they are expected, and
    their presence is informative rather than alarming.) **This reading must be
    taken before section D runs** — see FR-38a, which explains why the diagnostic
    would otherwise pollute its own measurement.
24. The index `VERSION` file. This is **one file at the top of the index tree**
    (`<app-assets>/index/VERSION`, written by
    `write_version_file(&paths.index_dir)` at
    `backend/src/search/indexer.rs:599`, current contents `1.0`) — **not** a
    per-language file, so it is reported once as a section-C header line rather
    than in the per-directory rows. Read it with the existing public helpers
    `read_version_file()` / `is_index_current()` and compare against
    `INDEX_VERSION` (`indexer.rs:609-645`, already used by
    `bridges/src/sutta_bridge.rs:3856`); do not re-read the file by hand. Report
    the value **and** whether it matches: a mismatch is a real "index stale or
    incomplete" input that the FR-34 verdict otherwise has no way to see.

**Section D — Tantivy open, as the app does it today**

25. For each index directory, perform the **current** open sequence and report
    **exactly which step fails**, with the full error:
    `MmapDirectory::open` → `Index::open` → `index.reader()`.
26. The output must attribute the failure to the correct step. The existing log
    conflates them behind one message (`searcher.rs:121`); this diagnostic must
    not. Per the research, `Index::open` takes **no lock** and is expected to
    succeed, with `index.reader()` being the failing step. (That expectation only
    holds where `meta.json` exists — see FR-40's note on the deliberate
    `open_or_create` divergence.)
    - **FR-26b.** Section D must **not** call `register_tokenizers`. It measures
      opens only and never queries; tokenizer registration is load-bearing solely
      in section E, where the `QueryParser` resolves `{lang}_stem` /
      `{lang}_normalize` off the `Index` at parse time (FR-28a). Registering in D
      implies a parity with E that does not exist.
    - **FR-26a.** The readers this section builds must be **dropped** as soon as each
      directory's steps are recorded. Section D deliberately reproduces the app's
      bare `index.reader()`, i.e. the default `ReloadPolicy::OnCommitWithDelay`,
      which spawns a 500 ms `meta.json`-polling thread per index for the reader's
      lifetime. Leaking one such thread per index per run would contradict Goal 4
      and success metric 6.

**Section E — Tantivy open through the candidate fix**

27. For each index directory, perform the same open **through the candidate
    `LenientLockMmapDirectory` wrapper** with `ReloadPolicy::Manual`, and report
    success or failure per step.
    - **FR-27a.** The report must state **which lock path the wrapper took** for each
      directory, with **three** outcomes rather than two: the inner `flock`
      succeeded; it fell back after an unsupported-operation errno; or it fell back
      after some *other* `IoError`. The third exists because
      `MmapDirectory::acquire_lock` opens the lock file *before* locking it, so an
      unwritable directory also yields `LockError::IoError` — and the FR-4 fallback
      would turn that into a clean-looking success on a genuinely broken volume.
      Without the distinction, section E cannot tell "the fix works" from "the fix
      hid the failure".

      **Report every distinct route the directory took, not just the last one.**
      One `index.reader()` reaches `acquire_lock` more than once —
      `open_segment_readers()` takes `META_LOCK` on every reader build
      (`reader/mod.rs:194`) — so a single overwritten slot can hide an early
      "fell back after some other `IoError`" behind a later "inner flock
      succeeded", which is exactly the pair this requirement exists to
      distinguish. `LenientLockMmapDirectory::lock_paths()` returns the distinct
      routes in the order first seen; `last_lock_path()` remains for callers that
      want one line.
28. On success, **run a real query** against each opened index and report the
    **hit count and elapsed milliseconds**. The query terms are **hard-coded**:
    **`nirodha`** and **`cessation`**. There must be **no input field**:
    hard-coded terms are reproducible across every report, and asking a user who
    is already confused about why search returns nothing to supply a term adds
    confusion for no diagnostic gain.
    - **FR-28d — run *both* terms against *every* index, and report both counts.**
      An earlier draft routed `nirodha` to "the Pāli indexes" and `cessation` to
      "the non-Pāli indexes". That rule is wrong on the shipped index tree, which
      is `suttas/{en,pli,san}`, `dict_words/{en,pli}`, `library/en`: it sends an
      English term at `suttas/san`, whose content is romanized Sanskrit, so a
      **populated** index returns zero — which FR-28b classifies as the
      *informative* case. Every healthy device with Sanskrit installed would
      report a fault. Running both terms everywhere costs one extra query per
      index, removes the language-classification guess entirely, and makes the
      row self-explanatory (`nirodha=N cessation=M`). A row is a hit if **either**
      term matched.
    - **FR-28a.** The query must target the **`content`** field via a single
      `QueryParser::for_index(index, vec![content_field])`. All three schemas
      define `content` (`{lang}_stem`) and `content_exact` (`{lang}_normalize`) —
      `backend/src/search/schema.rs:47-48` (suttas), `:99-100` (library),
      `:152-153` (dictionaries) — so one field selection covers all three alike.
      The live search's dual-field Must/Should boolean with its boost
      (`searcher.rs:515-525`) must **not** be reproduced: the question here is
      whether the index can be read at all, and the extra machinery adds risk
      without diagnostic value.
      Because `Index::open` reads the schema from `meta.json` rather than being
      handed one, `schema().get_field("content")` **can legitimately fail** (a
      foreign, truncated or older `meta.json`). That must be reported as its own
      attributed line — "schema has no `content` field" — and must not abort the
      section (FR-44).
    - **FR-28b.** A **zero hit count is expected, not a fault**, for any index the user may
      legitimately not have populated — the library index above all (most users
      import no books), but equally a `suttas/<lang>` or `dict_words/<lang>` index
      for a language whose content was never downloaded. The report must mark those
      rows as such. The informative case is a zero hit from an index that does
      contain documents.
    - **FR-28c — report `num_docs` per index.** FR-28b, FR-37 and the FR-34
      verdict all turn on "does this index actually contain documents", but
      nothing else in sections A–F measures it: C counts *files*, D and E measure
      *opens*. `reader.searcher().num_docs()` on the reader section E already
      holds answers it directly, in one line. It makes the expected-vs-unexpected
      zero-hit split a **measured** fact rather than an inference, and it is an
      independent proof that the read path works even if both query terms are
      unlucky.
29. This section is the point of the whole exercise: a non-zero hit count from a
    user's SD card is direct proof that the phase-2 fix works on real hardware,
    obtained before we commit to it. A failure here, with its error, tells us the
    wrapper is insufficient and what to design instead.
30. The wrapper used here must be the **real implementation** intended for phase 2
    — the same module, same fallback-lock behaviour — not a diagnostic-only
    approximation. Otherwise this section proves nothing.

**Section F — Live searcher state**

31. The counts currently held by the process-global `FULLTEXT_SEARCHER`
    (sutta / dict / library index counts) and the per-directory open failures
    recorded at startup. Both are properties of a searcher that has been built,
    so **"searcher not initialised this session"** must be reported as a state
    distinct from "0 indexes, 0 failures": `init_fulltext_searcher()` is lazy and
    mode-gated, and FR-41 forbids initialising it to find out. The failure list
    must be cleared whenever a searcher is (re)opened, so an entry recorded before
    a storage recovery is not reported as a live fault.
    - **FR-31a.** "Whenever a searcher is (re)opened" means **both** constructors.
      `FulltextSearcher` has two — `open()` (`searcher.rs:55`) and
      `open_from_dirs()` (`:78`) — and both call `open_indexes()` three times.
      Clearing only in `open()` leaves `open_from_dirs()` appending to a list that
      is never reset. Put the clear in a small helper that both call.
    - **FR-31b.** The "not initialised this session" state must be derived from
      `with_fulltext_searcher()` (`lib.rs:369`) returning `None`, **not** from
      `is_fulltext_searcher_ready()` (`lib.rs:358`), which returns `true` whenever
      the global is `Some` regardless of index count. That dishonesty is real, but
      fixing it is **phase-2 FR-20** and is out of scope here: correcting it in
      phase 1 would change `/health`'s `fulltext_searcher_ready` field and break
      Goal 4 / success metric 6.
32. The app version, platform, and Android API level where applicable.

**Section G — Plain-language verdict**

33. The summary must **open** with a short plain-language verdict of two or three
    sentences, before the technical sections, stating what was found and what it
    means for the user. The technical detail follows beneath it in full — the
    verdict summarises, it never replaces.
34. The verdict must be derived from the measured results, not guessed, and must
    cover at least these cases:
    - everything healthy;
    - the storage location does not support something the search index needs, and
      whether the candidate fix handled it (section E);
    - the index files are missing or incomplete;
    - the storage location is unreachable.
35. When the results do not match a known case, the verdict must say plainly that
    the result is unrecognised and asks the user to send the summary — never
    invent a diagnosis. An unrecognised pattern is exactly the case we most want
    reported.
36. The verdict must avoid `flock`, `mmap`, `Tantivy`, `FUSE` and errno names;
    those belong in the sections below it.
37. The verdict must not alarm a user whose problem turns out to be unrelated: if
    the storage checks pass, it must say so, and not imply a fault that was not
    found. Four measured states are **normal** and must never produce a fault
    verdict:
    - the searcher is not initialised this session (FR-31);
    - an index the user may not have populated returns zero hits (FR-28b);
    - stale `.tantivy-*.lock` files are present (FR-23);
    - `storage_path_state()` reports `absent` on **desktop**, which it always does
      (FR-13). Left ungated, this alone would fire the "storage location is
      unreachable" branch on every healthy desktop run.

### 4.4 Safety constraints

38. The diagnostics must be **read-only with respect to app data**. The only
    writes permitted are the diagnostic's own probe files.
    - **FR-38a — the one unavoidable exception, and how it is contained.**
      Section D's `index.reader()` reaches `MmapDirectory::acquire_lock`, which
      **opens (and therefore creates) `.tantivy-meta.lock` before locking it** —
      the same fact that FR-27a's third outcome rests on. On an index directory
      that has never been opened successfully, section D thus leaves behind lock
      files that were not there before, and the FR-39 `Drop` guards do not remove
      them (they are scoped to the diagnostic's own `simsapa-…` names). This is
      accepted rather than prevented: the files are zero-length, tantivy never
      deletes them anyway (FR-23), and any ordinary app startup creates them. Two
      requirements follow. (a) Section C's lock-file presence-and-age reading
      (FR-23) must be **collected before section D executes**, so the diagnostic
      does not report its own leftovers as pre-existing. (b) Where section C
      recorded a lock file as **absent** and the run created one, the report must
      say so in a single line, so a maintainer reading the report is not misled
      about the volume's prior state. The section ordering in the *report* already
      matches; the requirement here is that the **execution** order match too.
39. Every probe file must be removed on **every** exit path — success, failure and
    panic — via a `Drop` guard, in the style of `backend/src/storage_probe.rs`.
    Leaving litter on a user's card is a user-visible defect.
40. The diagnostics must **never create or modify an index**. All opens must use
    `Index::open`, never `Index::open_or_create`. Creating an index inside a
    user's index directory during a diagnostic would be a defect.
41. The diagnostics must **not disturb the live searcher**: it opens its own
    `Directory` and `Index` instances and must not touch, replace or reinitialise
    `FULLTEXT_SEARCHER`.
42. The summary must contain **no personal or sensitive data**: no sutta or
    dictionary content, no API keys, no bookmark or history data. Volume UUIDs and
    absolute paths are expected and necessary.
43. File-existence checks must use `try_exists()`, never `.exists()`, per the
    Android rule in CLAUDE.md.
44. A failure inside any one probe must not abort the run: each section must catch
    its own errors, print them, and continue. A diagnostic that crashes on the
    broken case is useless.

### 4.5 Code placement

45. The probe and report logic must live in the backend (Rust), e.g.
    `backend/src/storage_diagnostics.rs`, returning the summary as a `String`, so
    it is unit-testable off-device and reusable by phase 2. Its only caller is the
    bridge invokable behind the UI button (FR-4); there is no CLI or HTTP caller
    (Non-Goals).
46. The candidate wrapper must live in its **final** location
    (e.g. `backend/src/search/lenient_directory.rs`), so phase 2 changes call
    sites only.
47. New QML files must be added to the `qml_files` list in `bridges/build.rs`, and
    any new `SuttaBridge` method **and signal** needs a matching stub in
    `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` for `qmllint` — per
    CLAUDE.md. The nearest precedent **is** a model to copy: the file already has
    a "Search index signals" group declaring `rebuildSearchIndexProgress` (`:45`)
    and `rebuildSearchIndexCompleted` (`:46`), with the
    `rebuild_search_index()` function stub at `:264`. Add
    `storageDiagnosticsCompleted` to that same group. (An earlier draft of this
    PRD and of the task list claimed no such stub existed and that the signal
    block ended at `:39` — that was wrong; it runs to `:51`, with further signals
    at `:1157-1162`.)
48. QML logging must use the `Logger` component with a single concatenated string
    argument, never the `console` API.

## 5. Non-Goals (Out of Scope)

- **Fixing the bug.** No change to how search, indexing or storage selection
  behaves. The wrapper is written but wired **only** into the diagnostic.
- **Changing `ReloadPolicy` on the real search path.** That is a phase-2 item
  (fix PRD FR-17); here `Manual` is used only inside section E.
- **Any user-facing error message for failed fulltext search.** Phase 2.
- **Automatic or background running of the diagnostics.** It is user-initiated
  only.
- **Any entry point other than the UI button.** No CLI subcommand, and no
  `/health` or other HTTP exposure. We ask the affected user to press the button;
  that is the whole delivery mechanism. (`/health` would be actively wrong
  regardless — it is a readiness snapshot clients poll, and this report writes
  probe files and opens every index.)
- **Uploading anything.** The user copies text and sends it themselves. No
  network calls, no telemetry.
- **Localisation of the summary text.** It is a technical report for maintainers;
  English is correct.
- **The dictionary `.zip` / `scan_source` defect** seen in the same log — separate
  PRD.

## 6. Design Considerations

- The results dialog should reuse the visual language of the existing About and
  Database Validation dialogs; no new components beyond what is needed.
- The summary is **plain text with section headers**, not markup — it has to
  survive being pasted into an email or a chat message.
- Keep it short enough to paste comfortably. Per-language index rows should be
  one line each.
- Button labels are exactly **"Run Storage Diagnostics"**, **"Copy"**, **"Close"**.
- After **Copy** is pressed, give brief visual confirmation (e.g. the label
  changing to "Copied" for a moment), so the user knows it worked before they
  switch apps.

## 7. Technical Considerations

- **Everything in §4.3 sections B–E is phase-2 code.** FR-17/18 implement fix-PRD
  FR-8 and FR-34; FR-27/30 implement fix-PRD FR-1..FR-16. This PRD is the
  low-risk vehicle for writing and shipping them.
- Lock and mmap facts behind the design, all verified against `tantivy-0.25.0`:
  `MmapDirectory::acquire_lock` uses `fs4::fs_std::FileExt` (i.e. `flock(2)`) at
  `mmap_directory.rs:476`; the reader takes the blocking `META_LOCK` at
  `reader/mod.rs:194`; `Index::writer()` takes the non-blocking
  `INDEX_WRITER_LOCK` at `index/index.rs:545` and **discards the real errno**,
  mapping everything to `LockBusy`; index reads go through
  `memmap2::Mmap::map()` in `open_mmap`.
- Parsing `/proc/mounts` is readable on Android without special permission. The
  longest-prefix match is the correct way to find the governing mount.
- The diagnostic's own probe files should carry a distinctive name (as
  `storage_probe.rs` already does) so anything left by a killed process is
  identifiable as ours rather than mistaken for app data. Tantivy's GC only ever
  deletes files it manages, so an unmanaged foreign file in an index directory is
  left alone — cleanup is entirely the `Drop` guard's job (FR-39).
- **New dependencies:** `fs4 = "0.13"` and `memmap2 = "0.9"` (both already in
  `Cargo.lock` transitively via tantivy at 0.13.1 / 0.9.10, so no second copy),
  plus `libc` **solely** for the errno constants of FR-17. `fs4`'s default `sync`
  feature is what provides `fs4::fs_std::FileExt`; its free functions also cover
  FR-14's free/total space cross-platform, which is why no per-`cfg` `statvfs`
  helper is written.
- **The report has exactly one caller: the UI button.** There is no CLI
  subcommand and no `/health` exposure — see Non-Goals. The affected users are
  reached by asking them to press the button, so a second entry point would add
  surface area that nothing in §2 or §8 needs. Keeping the logic in the backend
  as a `String`-returning function (FR-45) is still right — it is what makes it
  unit-testable off-device and adoptable by phase 2 — but that is for testability,
  not for a headless caller.

## 8. Success Metrics

1. Both reporting users run the button, press **Copy**, and send back a summary —
   with no follow-up instructions needed from us.
2. The returned summaries state unambiguously: the filesystem type, the `flock`
   verdict with errno, and **the `mmap` verdict**.
3. Section E returns a **non-zero hit count** on at least one affected device —
   proving the phase-2 fix before it is written into the product. (A clean
   *failure* with a clear error also counts as success for this PRD: it redirects
   phase 2 before the effort is spent.)
4. Section D reproduces the known failure, attributing it to `index.reader()` and
   not to `Index::open` — confirming the research. (Meaningful only where
   `meta.json` exists; a missing `meta.json` fails at `Index::open` by design,
   per FR-40.)
5. No probe files remain on the user's volume afterwards.
6. On a healthy desktop install the diagnostics run reports all-clear — including
   *not* reporting the always-`absent` desktop storage state as a fault (FR-13,
   FR-37) — no other app behaviour changes, and the run leaves behind no live
   readers or `meta.json`-watcher threads (FR-26a).
7. `cd backend && cargo test` and `make qml-test` pass, with unit tests for the
   report builder and the mount-table parser using fixture input.

## 9. Decision Gate — what each outcome means for phase 2

| Section B `flock` | Section B `mmap` | Section E hits | Conclusion |
|---|---|---|---|
| unsupported | ok | > 0 | **Diagnosis and fix both confirmed.** Proceed with phase 2 as written. |
| unsupported | ok | 0 or error | Lock diagnosis right, wrapper wrong. Redesign phase 2 §4.1 from the reported error. |
| unsupported | **fails** | 0 | **Wrapper is necessary but not sufficient.** Phase 2 needs a non-mmap `Directory` (pread-backed or read-into-RAM `FileHandle`) — a much larger job, correctly scoped *before* it starts. |
| supported | ok | > 0 | The reporting user's fault is **something else**; re-triage from sections A, C, D. |

## 10. Resolved Questions

These were open at drafting and have been decided; recorded here so the reasoning
is not re-litigated during implementation.

1. **Auto-running the diagnostics on a zero-index startup — no.** The diagnostics
   are a **deliberate user action only**. This is already stated in Non-Goals and
   is not to be softened: nothing runs the probes in the background, on startup,
   or on a failed search. The cost of missing evidence from users who never press
   the button is accepted.
2. **Plain-language verdict — yes, alongside the technical detail** (see FR-33..37).
   The summary carries both: a short human-readable verdict at the top, then the
   full technical sections beneath it.
3. **The query terms are hard-coded** (FR-28). Reproducible across every report,
   and one less thing to explain to a user who is already confused about why
   search returns nothing. No input field in the dialog. The terms are
   **`nirodha`** and **`cessation`** (decided 2026-08-06, during the phase-3
   review of the task list). They are a translation pair, so the same passages
   answer both, and both are common enough that a zero hit count means the index
   is genuinely not readable rather than that the term was unlucky.

   **Amended 2026-08-06 (phase-5 review): both terms run against every index**,
   rather than routing by language (FR-28d). The original per-language routing
   sent `cessation` at `suttas/san`, whose content is romanized Sanskrit, so a
   healthy populated index returned zero and landed in FR-28b's *informative*
   bucket — a fault verdict on every device with Sanskrit installed. Running both
   everywhere costs one extra query per index and deletes the classification
   question.
4. **Ships on the regular release channel**, not as a beta-only build — the
   project is in closed testing, so the regular channel already reaches the right
   audience, and a release whose only visible change is a diagnostic button is
   acceptable there.

## 11. Open Questions

1. None outstanding. The query-term question raised during the phase-3 review has
   been decided — `nirodha` / `cessation`, recorded in FR-28 and §10.3, and
   amended by the phase-5 review to run **both** terms against every index
   (FR-28d).

   The phase-4 review (2026-08-06) raised no new open questions; every finding
   was resolvable against the source and has been folded into the requirements
   above (FR-10, FR-13, FR-14, FR-17, FR-18, FR-26a, FR-27a, FR-28a/b, FR-31,
   FR-37, FR-47, §7, §8.4/8.6) and into the task list's review-findings section
   (findings 11–21).

   The phase-5 review (2026-08-06) likewise raised no open questions, but did
   find one **incorrect** claim and several gaps, all resolved against the source
   and folded in above: FR-10a (modality), FR-10b (`extra_top_margin`), FR-10c
   (single owner for the run), FR-24 (the `VERSION` file is top-level, and
   `read_version_file()` already exists), FR-26b (no `register_tokenizers` in
   section D), FR-28a (the `get_field` failure path), FR-28c (`num_docs`),
   FR-28d (both terms everywhere), FR-31a (both constructors), FR-31b (do not
   "fix" `is_fulltext_searcher_ready()` here), FR-38a (section D creates lock
   files), and FR-47 (the `rebuildSearchIndexCompleted` stub **does** exist). The
   corresponding task-list findings are 22–32.

2. **Decided 2026-08-06 — no CLI subcommand and no `/health` exposure.** §7
   previously floated a CLI subcommand as near-free, and the phase-5 review had
   worked out the `init_app_globals()` mechanics for it. Both are now Non-Goals:
   the users this PRD serves are reached by asking them to press the button, so a
   headless entry point earns nothing and only adds surface area. The backend
   function still returns a `String` (FR-45) — for testability and for phase 2,
   not for a second caller.

   Anything discovered during implementation should be added here rather than
   resolved silently.

3. **The mmap probe (FR-18) can take the process down without returning, and
   nothing can catch it.** Raised by the phase-6 review (2026-08-06, after tasks
   1.0–3.0 were implemented). A read *through* a mapping that faults — a
   truncated file, some FUSE modes — raises **SIGBUS**, which is a signal, not a
   panic: the `catch_unwind` of task 6.2 cannot intercept it, the completion
   signal is never emitted, and the app dies. This is unlikely (a `direct_io`
   FUSE mount normally fails at `mmap()` itself with `ENODEV`, which *is*
   reported as an ordinary error), but if it happens at all it happens on
   precisely the devices this PRD was written for.

   **Resolved as far as it can be, without a signal handler:** the probe now
   logs `about to memory-map <file> (<n> bytes)` at INFO *before* touching the
   mapping. Since the summary is only written to the log when the run completes
   (FR-11), that line is the only thing a crashed run would leave behind — and
   it names the file and the volume, which is the answer. Installing a `SIGBUS`
   handler to convert the fault into a reported error was considered and
   rejected: a process-wide signal handler is a far larger behaviour change than
   this PRD's Goal 4 allows, for a failure mode we have not once observed.

4. **`FlockSupport` is cached for the process lifetime whatever the verdict**
   (fix-PRD FR-7), so a transient `Error` — a directory that happened to be
   unwritable at first probe — sticks for the session. Left as is, deliberately:
   only `Unsupported` changes the wrapper's routing, and "this filesystem does
   not implement advisory locking" is not a transient property. A stale `Error`
   therefore costs at most one failing syscall per lock, never a wrong route,
   and a restart re-probes. Recorded at `flock_support_for_dir()` and to be
   repeated in `docs/storage-diagnostics.md` (task 8.4).
