# Tasks — Fulltext search fix + dictionary import overhaul (the "make it work" build)

**Date:** 2026-08-25
**Goal:** the next build sent to the reporting ChromeOS user has **working
fulltext search** and a **working dictionary import**, not another diagnostic.

## Source PRDs — read these, do not re-derive them

| PRD | What this task list takes from it | Status there |
|---|---|---|
| `tasks/2026-08-05-193727-prd---fulltext-index-on-filesystems-without-flock.md` | §4.1–§4.6 (FR-1…FR-36) — the whole fulltext fix. **§10 records what phase 1 measured.** | **Unblocked 2026-08-25.** Implement as written. |
| `tasks/2026-08-05-201545-prd---run-storage-diagnostics.md` | §12 — the returned measurements the fulltext fix now rests on. | **Complete.** Shipped, reported, decision gate row 1. |
| `tasks/2026-07-31-180502-prd---picker-url-handling-and-chromebook-import-failure.md` | §4A.6 (phase-1 results), §4A.7 (E-1…E-17), §5.2–§5.5 (Reqs. 8–12, 15, 17b–26) | Phase 1 complete; **most of phase 2 (§5) stays blocked** — see §0.4 below. |

Sibling task lists, for the shipped code this builds on:
`tasks/2026-08-05-201545-tasks-run-storage-diagnostics.md` (the wrapper and the
probes, in their final locations) and
`tasks/2026-07-31-180502-tasks-picker-url-handling-and-chromebook-import-failure.md`
(`picker_url.rs`, the raw picker, the provider reader).

---

## 0. Session context — the facts this plan rests on

Written down because they were established by reading logs and Qt source in one
session, and a new session will otherwise re-derive them or, worse, guess.

### 0.0 Where the evidence lives — exact paths and line numbers

All under
`/home/gambhiro/prods/apps/simsapa-ng-project/feedback-and-bug-reports/rechromebookstoragetesting/`
— note that is **one level above the `simsapa` repo root**, i.e.
`../feedback-and-bug-reports/…` from the working directory, not inside it.

**Chronological order is by the UTC timestamps *inside* the files, not by
filename** — the filenames are rotation times and put session B before session A.

| File | Contains | Lines |
|---|---|---|
| `log.2026-08-03T13-04-00.txt` | **The original failing import**, on the *pre-diagnostic* build. Two attempts, both `ERROR: scan_source failed: Path not found: ` with **nothing after the colon**. Also the six `flock` index-open failures. | `:127-128` (import), `:38-78` (flock) |
| `log.2026-08-25T21-21-50.txt` | Session **A** — File Selection Test runs 1–3: `14:14:59` cancelled, `14:15:46` `gd.zip`, `14:17:18` `gd.zip`. | `:331-487` |
| `log.2026-08-25T14-23-46.txt` | Session **B** — runs 1–2: `14:24:46` **`mdict.zip`**, `14:27:51` `gd.zip`. | `:138-409` |
| `Screenshot_2026-08-25_21.17.20_-_File_Selection_Test.png` | The "returned a file from another app" dialog. Local time → session A run 3 at `14:17:18.201Z`, i.e. **`gd.zip`**. | — |
| `Screenshot_2026-08-25_21.24.38_-_File_picker.png` | The ChromeOS Files picker, `all-dictionaries-mdict.zip` highlighted, "Apri" not yet pressed — 8 s before session B run 1. | — |

**The storage-diagnostics summary is NOT in that folder.** The user pasted it as
text; it is reproduced in full in
`tasks/2026-08-05-201545-prd---run-storage-diagnostics.md` **§12**, which is the
only record. Do not go looking for a file.

Useful greps:

```sh
cd ../feedback-and-bug-reports/rechromebookstoragetesting/
# every measured field of every run (56 lines)
grep -nE "run [0-9]+ (begin|end)|raw_uri:|provider_|qurl_of_raw" log.2026-08-25T*.txt
# the per-run outcome lines the user saw on screen (10 lines)
grep -nE "File Selection Test: (starting|completed)" log.2026-08-25T*.txt
# the original failure, empty path after the colon
grep -n "scan_source failed" log.2026-08-03T13-04-00.txt
```

The greps above were run 2026-08-25 and the line counts are what they returned;
a different count means the folder has changed.

**Developer-machine paths that this plan needs** (do not re-derive):

- `SIMSAPA_DIR` for tests:
  `/home/gambhiro/prods/apps/simsapa-ng-project/bootstrap-assets-resources/dist/simsapa`
- the real dev index tree (task 3.0's benchmark corpus):
  `…/dist/simsapa/app-assets/index/` — verified present 2026-08-25: `VERSION`
  plus `suttas/{en,hu,pli}`, `dict_words/`, `library/`. Note this machine's tree
  is **not** the user's (theirs has `suttas/san` and no `hu`), so the benchmark
  must enumerate what it finds rather than hard-coding language keys.
- appdata DB: `…/dist/simsapa/app-assets/appdata.sqlite3`
- Qt sources for the file-dialog helper (two copies, identical):
  `/home/gambhiro/Qt/6.9.3/Src/qtbase/src/plugins/platforms/android/qandroidplatformfiledialoghelper.cpp`
  and `/mnt/proj/Qt/6.9.3/Src/…`
- `QJniObject::callMethod` exception clearing:
  `/home/gambhiro/Qt/6.9.3/Src/qtbase/src/corelib/kernel/qjniobject.h:132-152`
- tantivy 0.25 sources: `~/.cargo/registry/src/*/tantivy-0.25.0/`
- **Never invoke a bare `qmake6`/`qmllint`** — `source scripts/qt-env.sh` first
  (CLAUDE.md); the system Qt is 6.11.1 and is not what this project targets.

### 0.1 The reporting device

ChromeOS **151.0.7922.168** (64-bit), Simsapa **1.0.0-alpha.6**, Android
**API 33**. Storage on an external **`fuse`** volume (statfs magic `0x65735546`),
165.0 GiB free of 233.1 GiB, mount
`/storage/E297186276AA7E917DC8E6AC2FFA3BF32E0D48BB`. **It is not an SD card and
it is not slow.**

**The device clock is UTC+7; the log timestamps are UTC.** That 7-hour offset is
what lets the screenshots be placed against log lines — the
`Screenshot_2026-08-25_21.17.20` file is the run logged at `14:17:18.201Z`.
Needed again for the next report.

### 0.2 Fulltext — solved, and proven on the device

Decision-gate **row 1**: `flock` **unsupported(38 ENOSYS)**, `mmap` **ok**,
section E returned **real hits on every populated index**.

- `mmap` worked on an 18,128,732-byte `.pos` file with reads forced at offsets
  0, 9,064,365 and 18,128,731 — i.e. faulting well past page 0, the case a
  `direct_io` FUSE mount fails. **The non-mmap `Directory` contingency
  (fix-PRD FR-36) is not needed and must not be written.**
- `LenientLockMmapDirectory` opened all six indexes; every one reported the lock
  route as *"fell back after an unsupported-operation errno (38 ENOSYS)"* —
  never FR-27a's third outcome, so the fallback is working around an
  unsupported primitive, not masking an unwritable volume.
- Section D attributed the failure to `index.reader()`, **not** `Index::open`,
  confirming the research: `Index::open` takes no lock.
- Per-index results, for reference when judging the benchmark's realism:

  | Index | `num_docs` | nirodha | cessation | reader build |
  |---|---|---|---|---|
  | suttas/en | 10722 | 14 | 1317 | 233.0 ms |
  | suttas/pli | 10649 | 731 | 0 | 627.7 ms |
  | suttas/san | 0 | 0 | 0 | 77.1 ms |
  | dict_words/en | 13587 | 7 | 46 | 161.9 ms |
  | dict_words/pli | 539569 | 2227 | 555 | 406.6 ms |
  | library/en | 322 | 32 | 119 | 529.1 ms |

### 0.3 The import — what is proven and what is not

Five File Selection Test runs (one cancelled, four real). **All four real picks
were clean:**

```
raw_branch:            intent-getData
raw_uri:               content://org.chromium.arc.volumeprovider/0000…CAFEF00D2019/Documenti/Dizionari/all-dictionaries-gd.zip
qurl_of_raw_is_valid:  yes          encoding_differs:  no
url_scheme:            content      url_host:          org.chromium.arc.volumeprovider
provider_opened:       true         provider_size:     180735851
provider_bytes_read:   4194304 (cap reached)
provider_open_ms:      2–8          provider_read_ms:  6–15
staging_roots_differ:  no
```

Consequences, each of which changes what must be built:

1. **The empty-URL bug did not reproduce through the raw picker.** The ARC
   provider returns an ordinary, **completely unencoded** `content://` URI that
   `QUrl(QString)` accepts. §2.1a's leading hypothesis is refuted for these URIs.
2. **Defects A, B and C have zero observed instances.** Defect B has none on any
   device now (an Android 16 phone measured the same). ChromeOS emits
   `content://`, not `externalfile:` — Q2 answered.
3. **Defect D.2 is false on ARC too** (`staging_roots_differ: no`). **Req. 20 is
   a non-issue.** Req. 21a keeps only its "nothing ever deletes the staged copy"
   half.
4. **The loss is inside Qt's `FileDialog`.** Comparing
   `~/Qt/6.9.3/Src/qtbase/src/plugins/platforms/android/qandroidplatformfiledialoghelper.cpp`
   against `cpp/android_raw_pick.cpp` leaves exactly four deltas:
   - **the MIME filter** — `nameFilters: ["StarDict archives (*.zip)"]` →
     `nameFilterExtensions()` → `QMimeDatabase::mimeTypeForFile("*.zip", MatchExtension)`
     → `setType("application/zip")` **and** `EXTRA_MIME_TYPES` (`:146-179`).
     **Leading suspect.** The diagnostic sets `*/*` and no extras;
   - `takePersistableUriPermission()` (`:47`, before the append) on an intent
     that never sets `FLAG_GRANT_PERSISTABLE_URI_PERMISSION` (`:181-187`), so it
     is expected to throw `SecurityException` on every pick. **Probably not the
     cause** — `QJniObject::callMethod<void>` clears pending exceptions
     (`qjniobject.h:141-144`) — but it is a real Qt defect, worth not
     re-investigating from scratch;
   - `EXTRA_INITIAL_URI` via `setInitialDirectoryUri()` (`:227`);
   - the `getClipData` branch (`:56-71`), which calls
     `m_selectedFile.constFirst()` unconditionally after the loop — a crash if
     `getItemCount()` is 0.
5. **`all-dictionaries-gd.zip` is confirmed StarDict** (source known; `-gd` names
   GoldenDict, which *reads* StarDict). It should import once a path arrives.
   `all-dictionaries-mdict.zip` (185,525,740 bytes) never could.
6. **The user could not tell the two files apart** — they alternated across five
   runs while trying to identify "the file that would not import". That is the
   user-facing cost of the silent rejection, and the reason task 7.0 exists.
7. **The on-screen line the user reported as "the error" was the success
   message.** `outcome_line()` (`backend/src/picker_url.rs:843`) is a function of
   the *input*, not the result — it never sees whether the read worked — and its
   `Provider` arm names a mechanism with no success word. Every one of those runs
   logged `success = true`. See task **6.6**.

### 0.4 What stays out of scope, and why

- **Picker-URL PRD §5's four-call-site migration** — Reqs. 1–7a, 13, 14, 14a,
  16, 27–29. **Req. 15 (the `scan_source` URL-scheme rejection) is in scope**
  (task 7.3), as is **Req. 26** (no new permission — task 6.7), and **Req. 30**
  (zip path traversal — task 5.4); do not read this bullet as excluding them.
  Still blocked: none of Defects A–D is demonstrated. This build
  fixes the *dictionary* import specifically, by the fallback of task 6.0. The
  shared-resolver refactor remains correct engineering with no known victim, and
  belongs in its own build where a desktop regression is not riding alongside a
  fulltext fix.
- **Fulltext PRD §4.7 — the removable-volume performance notice (FR-37…FR-45).**
  Deferred deliberately: the only affected volume ever measured is **fast**
  (fix-PRD §10.2), so the notice would assert something unmeasured. It shares no
  code with the fix and blocks nothing.
- **MDict *reading*.** Dropped on the user's instruction. Only the *naming* of a
  non-StarDict archive is in scope (task 7.0).
- **`is_fulltext_searcher_ready()`'s meaning** was deliberately left alone in
  phase 1 (diagnostics FR-31b) to avoid changing `/health`. **It is in scope
  now** — fix-PRD FR-20, task 2.4.

---

## 1. Component analysis — what has to exist, and what blocks what

```
FULLTEXT  (independent of everything below; highest user value; ship-blocking)
  3.1 baseline benchmark   ← MUST run before 1.0 lands, or there is no "before"
  1.0 call sites           wrapper + Index::open + ReloadPolicy::Manual
  2.0 honest reporting     counts, failures, /health, Validation row, search UI
  3.2-3.7 benchmark        proves 1.0 cost nothing

DICTIONARY IMPORT  (all four touch one flow)
  4.0 off the UI thread    ← first: it defines the staging signal surface
  5.0 extract once + clean ← 5.1's entry-list read is what 7.0 needs
  6.0 picker fallback        depends on 4.0's staging signals
  7.0 non-StarDict message   depends on 5.2's typed probe result

8.0 Docs + tests + build   ← last
```

**Order that matters.** 3.1 (baseline benchmark) must run **before** any of 1.x
lands, or there is nothing to compare against. 4.0 introduces the staging
signals that 5.0 and 6.0 both report through, so it goes first among the import
tasks. 7.0 needs 5.0's typed failure reason to have something to say.

---

## 2. Notes on the existing codebase (assessed before planning)

Verified by reading, 2026-08-25. Line numbers are from that reading.

### Fulltext

- `backend/src/search/lenient_directory.rs` **already exists in its final
  location** and is complete: `FlockSupport`, `probe_flock_support()`,
  `flock_support_for_dir()` (cached), `FallbackLock`, `LockPathTaken`,
  `LenientLockMmapDirectory::open()`, `lock_paths()`, `last_lock_path()`, and
  `impl Directory`. Phase 2 **wires it up**; it does not write it.
- `backend/src/search/searcher.rs:156-160` — `open_single_index` still uses
  `MmapDirectory::open` + `Index::open_or_create` + a default-policy
  `index.reader()`. This is the read path; all three are wrong per FR-9, FR-11,
  FR-17.
- `backend/src/search/indexer.rs:34-35` (`open_or_create_index`, serving six
  builders at `:50, :142, :237, :338, :419, :679`), `:764-771` (dictionary index
  deletion) and `:814-821` (`list_indexed_source_uids_in_dict_index`) — the
  three write-path sites of FR-9. `Index::open_or_create` **stays** here.
- **The failure list already exists** (phase 1 shipped FR-19/FR-22 as
  diagnostics): `SEARCHER_OPEN_FAILURES` at `backend/src/lib.rs:371`, with
  `record_searcher_open_failure()` / `clear_searcher_open_failures()` /
  `searcher_open_failures()`, recorded at `searcher.rs:138` and cleared by
  `begin_open_session()` (`searcher.rs:62`), which **both** constructors call
  (FR-31a satisfied). Phase 2 reads it, it does not build it.
- `backend/src/lib.rs:331` `init_fulltext_searcher()` (lazy, mode-gated),
  `:344` `reinit_fulltext_searcher()` — logs a bare `"Fulltext searcher
  initialized"` at INFO with no counts (FR-21), `:408`
  `is_fulltext_searcher_ready()` — returns `is_some()` regardless of index count
  (FR-20), feeding `bridges/src/api.rs:1824`'s `/health`.
- `assets/qml/FulltextResults.qml:335-350` — the `empty_state` `Text`. It
  already carries a **precedent for a non-generic empty state** (the
  `snippet_exclude_terms` branch), so FR-23…FR-26 extend an existing pattern
  rather than inventing one. `db_ready` (`:107`) is the gate that stops it
  firing during load.
- `assets/qml/DatabaseValidationDialog.qml:139` — `databases` is
  `[["appdata", "Appdata"], ["dictionaries", "Dictionaries"]]`; failures
  accumulate at `:213-221`. The signal is
  `database_validation_result(database_name, is_valid, message)`
  (`bridges/src/sutta_bridge.rs:895`), emitted at `:2133`, `:2189`, `:2247`.
- **There is no benchmark infrastructure**: no `backend/benches`, no
  `[[bench]]`, no `criterion`. Task 3.0 creates its own.

### Dictionary import

- **The import stage already runs off the UI thread and already has progress and
  abort.** `bridges/src/dictionary_manager.rs`: `import_zip` (`:306`),
  `import_dir` (`:381`), `scan_source` (`:451`) each `thread::spawn`, reporting
  through `import_progress` / `import_finished` / `import_failed` /
  `import_cancelled` / `scan_finished` / `scan_failed` (`:130-152`).
  `assets/qml/DictionariesWindow.qml:130-300` drives a full sequential batch with
  a progress frame (`views_stack` index 2), an Abort button and a summary frame.
  **Do not rebuild this.**
- **The one UI-thread blocker is the staging copy.**
  `SuttaBridge.copy_content_uri_to_temp` is a plain synchronous invokable
  (`bridges/src/sutta_bridge.rs:3954`) called straight from
  `DictionaryImportDialog.qml:181` inside `FileDialog.onAccepted`. It reaches
  `copy_content_uri_to_temp_file` (`cpp/utils.cpp:640-700`), which does
  `QByteArray data = source.readAll()` (`:678`) — **the whole 180 MB into one
  buffer on the GUI thread**, then one `dest.write(data)`.
- **The scanning frame is indeterminate with no cancel and no byte count**
  (`DictionaryImportDialog.qml:375-410`), which is what a user watching a
  177 MB extraction sees.
- **`set_keep_screen_on` is called nowhere in the dictionary flow.** Verified:
  zero hits in `DictionariesWindow.qml` and `DictionaryImportDialog.qml`. This
  violates the standing CLAUDE.md rule and means the device can suspend
  mid-import.
- **The archive is extracted twice.** `probe_zip_candidate`
  (`backend/src/dictionary_manager_core.rs:459-480`) does
  `archive.extract(&extract_dir)` into a `tempfile::tempdir_in(simsapa_dir)` just
  to read the `.ifo` and the index count; `import_user_zip` (`:220-247`) extracts
  the same archive again.
- **`probe_zip_candidate` returns `None` on every failure** — non-StarDict, bad
  zip, full disk, extraction error — and `scan_source` (`:498`) then returns
  `Ok(vec![])`, which QML renders as *"No StarDict dictionaries were found in
  the chosen source."* (`DictionaryImportDialog.qml:154`).
- **Cleanup.** `tempfile::TempDir` auto-deletes on drop, so a normal or errored
  return is clean — but `archive.extract()` is **not cancellable** (the `cancel:
  &AtomicBool` is only consulted inside `import_stardict_as_new`), and a killed
  process leaves `simsapa-stardict-*` / `simsapa-stardict-probe-*` directories in
  `SIMSAPA_DIR` forever. **There is no startup sweep for them** (grep:
  the prefixes appear only at `dictionary_manager_core.rs:230` and `:462`).
- **The staged `.zip` is never deleted.** `delete_temp_import_folder`
  (`bridges/src/sutta_bridge.rs:3978`) is called only by
  `DocumentImportDialog.qml`, and it wipes the **shared** root — Defect D's
  claim 1, which the 08-25 measurements did **not** retire.
- `zip = { version = "2", default-features = false, features = ["deflate"] }`
  (`backend/Cargo.toml:63`) — Req. 30's path-traversal check applies to 2.x.
- `ANALYZE` is already correct: `dictionary_manager_core.rs:189` after a
  successful import and `:591` after a delete. **Do not add another** (see
  `docs/user-data-and-sqlite-analyze.md`).

### Picker

- `backend/src/picker_url.rs` — the Qt-free classifier (`PickerBranch`,
  `PickerUrlFacts`, `classify`, `FileSelectionTestInput`, `outcome_line` at
  `:843`, `run_and_log_file_selection_test` at `:874`). Req. 29's module, already
  in its final location.
- `cpp/android_raw_pick.cpp` — our own `ACTION_OPEN_DOCUMENT`, request code
  **51305** (`:76`), `CATEGORY_OPENABLE`, `setType("*/*")`, no `EXTRA_MIME_TYPES`
  (`:162-174`). Delivers through `raw_document_pick_result_c()` on **every** path
  including the two early failures. **The only file including private Qt API**
  (`QtCore/private/qandroidextras_p.h`); `Qt6::CorePrivate` is linked on Android
  only.
- `bridges/src/sutta_bridge.rs:75-150` — `FILE_SELECTION_TEST_THREAD` (a
  process-global `Mutex<Option<CxxQtThread>>` registered when a raw pick starts),
  `picker_url_facts_from()`, `spawn_file_selection_test()` with `catch_unwind`.
  `start_file_selection_test_raw_pick` at `:4327`; signal
  `fileSelectionTestCompleted` at `:997`. **There is exactly one global slot, so
  a second caller needs a discriminator — see task 6.3.**
- The provider reader `probe_document_uri` lives in `backend/src/android_saf.rs`
  and uses `ContentResolver.openInputStream` with the `to_encoded()` rule. The
  import still uses `QFile(content_uri)` (`cpp/utils.cpp:662`).

---

## 3. Relevant Files

**Fulltext**

- `backend/src/search/searcher.rs` — `open_single_index` (`:149`): wrapper,
  `Index::open`, `ReloadPolicy::Manual`. Also the per-area open counts for FR-21.
  **Done (1.1–1.3).**
- `backend/src/search/indexer.rs` — the four write-path sites (`:34`, `:764`,
  `:814`, and by inheritance the six builders). **Done (1.1);** `open_or_create`
  deliberately kept — these are write paths.
- `backend/src/search/lenient_directory.rs` — no behaviour change; gained
  `flock_probe_count()` / `flock_probe_count_for_dir()` (3.4's option (a),
  always compiled) and three tests covering the fallback route. **Done (1.5, 1.6).**
- `bridges/src/dictionary_manager.rs` — `start_reconcile()` now calls
  `reinit_fulltext_searcher()` after mutating the dict index. Required by 1.3;
  it was the one site relying on the reader's removed auto-reload. **Done (1.4).**
- `backend/src/lib.rs` — `reinit_fulltext_searcher()` (`:344`) counts + ERROR on
  zero; `is_fulltext_searcher_ready()` (`:408`) honesty; a new accessor for the
  per-area counts so QML and `/health` read one source. **Done (2.1–2.4, 2.8):**
  `fulltext_index_counts()`, `fulltext_status_json()`, and the
  `log_storage_capability_verdicts()` call.
- `backend/src/fulltext_status.rs` — **new (2.1, 2.3, 2.7).** The single place
  that turns per-area counts + recorded failures into one verdict and one
  plain-language sentence, consumed by the search UI, Database Validation and
  `/health` alike. `FulltextState` has four variants so `files_not_found` can
  never be reported as `could_not_open`. Its `no_jargon_in_user_facing_strings`
  test is what enforces 2.7 for every string the feature can emit.
- `backend/src/search/searcher.rs` — **also (2.1):** `FulltextAreaStatus` /
  `FulltextIndexCounts`, captured at open time. `open_indexes()` now returns
  whether the directory existed, which is what makes 2.3's distinction possible
  at all. The old tuple-returning `index_counts()` was **replaced**, not
  duplicated (two callers updated in `storage_diagnostics.rs`).
- `backend/src/storage_probe.rs` — FR-31…FR-33's `flock`/`mmap` verdicts
  recorded (demote-only contract unchanged; a `flock` failure alone **must not**
  demote).
- `bridges/src/sutta_bridge.rs` — the "fulltext" `database_validation_result`
  row (near `:2247`).
- `bridges/src/api.rs:1801,1824` — `/health`'s `fulltext_searcher_ready`, plus
  per-area counts.
- `assets/qml/DatabaseValidationDialog.qml:139,213-221` — the new row. **Done
  (2.5).** The `"fulltext"` result is intercepted in `onDatabaseValidationResult`
  and **kept out of** `validation_results`: it is not in `expected_databases`,
  so letting it in would have broken the three-database completion state
  machine, and it is not downloadable, so it must never reach
  `get_failed_downloadable_list()` (which would build a bogus asset URL). It
  renders as its own section beside "Search index:", and feeds `has_any_failure`
  only.
- `assets/qml/FulltextResults.qml:335-350` — the "index could not be opened"
  empty state. **Done (2.6).** Gated on `state === "could_not_open"`, never on a
  zero result count, which is what satisfies FR-26. The status arrives via a
  **`fulltext_status_fn` callback** supplied by the parent, matching the
  existing `new_results_page_fn`: this component's
  `import com.profoundlabs.simsapa` is deliberately commented out so it stays
  usable in QML preview, and a direct `SuttaBridge` call there produced a new
  `qmllint` unqualified-access warning naming the file.
- `assets/qml/SuttaSearchWindow.qml` — supplies `fulltext_status_fn` and
  `search_area` to `FulltextResults`.
- `backend/tests/test_lenient_directory_benchmark.rs` — **new (3.2–3.6).** Two
  tests: the bare-vs-wrapper ratio benchmark (which also carries 3.4's
  per-directory probe-count assertion) and the Linux-only watcher-thread check
  for `ReloadPolicy::Manual`. Enumerates the index tree, never creates one,
  skips with a message when `SIMSAPA_DIR/app-assets/index` is absent.
- `docs/fulltext-index-storage-and-file-locking.md` — **new (3.7).** Currently
  holds the framing and the benchmark record only; task 8.2 writes the mechanism
  sections.

**Dictionary import**

- `backend/src/dictionary_manager_core.rs` — `probe_zip_candidate` (`:459`),
  `import_user_zip` (`:220`), `scan_source` (`:498`), `locate_stardict_dir`
  (`:366`), `find_ifo_stem_in` (`:389`).
- `backend/src/picker_url.rs` — `outcome_line` (`:843`); the staging-root helpers
  (`:272-293`).
- `backend/src/import_staging.rs` — **new (4.1–4.3).** The whole
  platform-independent staging half: `StagingRequest` / `StagedFile` /
  `StagingError` (stable `code` + `step` + `message`), `staging_dir` (per
  feature), `sanitize_staged_file_name`, `ensure_free_space`,
  `copy_stream_to_file` (1 MB chunks, cancel-aware), `reject_empty`,
  `stage_picked_url`. Qt-free and unit-tested off-device; 9 tests.
- `backend/src/android_saf.rs` — `probe_document_uri`; **new (4.1, and 6.4
  early): `document_metadata` and `copy_document_to_path`**, the chunked
  `ContentResolver.openInputStream` reader, plus the shared `parse_uri` helper.
  Cross-checked with `cargo check --target aarch64-linux-android`.
- `bridges/src/dictionary_manager.rs` — **new (4.1, 4.5):**
  `stage_picked_file(&QUrl)` + `abort_staging()`, the `stagingProgress` /
  `stagingFinished` / `stagingFailed` signals (each with an explicit
  `#[cxx_name]`), and the `staging_cancel` flag on `DictionaryManagerRust`.
  Progress is throttled to 100 ms rather than emitted per chunk.
- `bridges/src/sutta_bridge.rs` — `qurl_to_local_path` is now `pub(crate)` (the
  staging bridge needs the same Windows drive-letter handling), and
  `copy_content_uri_to_temp` carries the 4.7 comment naming its replacement.
- `bridges/src/sutta_bridge.rs` — `copy_content_uri_to_temp` (`:3954`),
  `delete_temp_import_folder` (`:3978`), the raw-pick plumbing (`:75-150`,
  `:4327`).
- `cpp/utils.cpp` — `copy_content_uri_to_temp_file` (`:640-700`),
  `get_import_staging_root` (`:590`).
- `assets/qml/DictionaryImportDialog.qml` — **done (4.4–4.6).** Four named
  frame indices, the new `frame_copying` frame, `begin_staging` /
  `cancel_staging` / `abandon_scan`, the three `onStaging*` handlers, and two
  keep-screen-on holders. `file_dialog.onAccepted` is now one line into
  staging — it no longer inspects the URL scheme itself and no longer calls
  `SuttaBridge.copy_content_uri_to_temp`.
- `assets/qml/DictionariesWindow.qml` — **done (4.6).** `AssetManager` added;
  `dictionary-import-batch` held across `start_batch` → `finish_batch`.
- `assets/qml/com/profoundlabs/simsapa/{SuttaBridge,DictionaryManager}.qml` —
  stubs for every new invokable **and signal** (CLAUDE.md).
- `bridges/build.rs` — any new `.qml` file, in the `"../assets/qml/<Name>.qml"`
  form exactly.

**Docs**

- `docs/fulltext-index-storage-and-file-locking.md` — **new** (fix-PRD §7).
- `docs/dictionary-import-pipeline.md` — **new** (task 8.3).
- `docs/file-selection-test.md` — §5.3 already records the returned report;
  update again if task 6.0 changes the picker.
- `docs/storage-diagnostics.md`, `docs/relocated-storage-recovery.md`,
  `docs/search-snippet-highlight-pipeline.md`, `docs/android-file-saving-saf.md`,
  `CLAUDE.md`, `PROJECT_MAP.md` — cross-links.

---

## 4. Notes

- Rust tests: `cd backend && cargo test`; QML: `make qml-test`; build:
  `make build -B`.
- **Do not run the GUI to test** (CLAUDE.md). Compile-verify and unit-test; the
  on-device check is the user's.
- `cargo check` does **not** compile `android_saf.rs` (it is
  `#[cfg(target_os = "android")]`). Cross-check it explicitly — the recipe is in
  `tasks/2026-07-31-180502-tasks-picker-url-handling-and-chromebook-import-failure.md`
  §Notes, and the same hole applies to `#ifdef Q_OS_ANDROID` C++.
- `try_exists()`, never `.exists()`.
- QML logging: `Logger { id: logger }`, **single concatenated string**, never
  `console`.
- New C++ logging: `log_info_c()` / `log_error_c()`, never `qInfo()`/`qWarning()`.
- `crate::queue_or_log()` for every `qt_thread.queue()`; never `.unwrap()` or
  `let _ =`.
- Any new `Dialog` states its own width clamped to `Overlay.overlay`, and uses
  `DialogHeader` if it has both a title and wrapping text.
- **Timing assertions:** the project has known drift in absolute-time budgets.
  Every timing check added here must be a **ratio between two measurements taken
  in the same run on the same machine**, never an absolute millisecond budget.
- **Every new `#[qsignal]` needs an explicit `#[cxx_name = "camelCase"]`** —
  cxx-qt does not derive it, and QML's `on…` handler silently never fires
  without it. Every signal in `bridges/src/dictionary_manager.rs:130-178` has one.
- **Every new bridge invokable *and signal* needs a stub** in
  `assets/qml/com/profoundlabs/simsapa/{SuttaBridge,DictionaryManager}.qml`, or
  `qmllint` fails (CLAUDE.md). `SuttaBridge.qml:45-48` groups signals; `:283`
  shows an invokable stub.
- **Structured bridge results are returned as a JSON string** — the project's
  established convention (`scan_source`, `get_word_json`). Do not invent a new
  return channel for the typed probe result of task 5.2.
- **Integration tests in `backend/tests/` link the lib without `cfg(test)`.**
  Anything a test needs to observe must be `pub` and unconditionally compiled.
  See task 3.4.
- **`cargo test` runs tests in parallel in one process** — process-global state
  (`SEARCHER_OPEN_FAILURES`, `FULLTEXT_SEARCHER`, the `flock` support cache,
  thread counts) is shared across tests. Assert on names and deltas, not on
  absolute global counts.

## 5. Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, check it off in this markdown file by
changing `- [ ]` to `- [x]`. Update the file after completing each sub-task, not
just after completing an entire parent task.

---

## Tasks

### 1.0 [x] Wire the lenient directory into the real search paths (fix-PRD FR-9, FR-11, FR-16, FR-17)

**Specs to keep in mind.** The wrapper is written and proven (§0.2); this task is
call-site surgery. On a normal filesystem `probe_flock_support` returns
`Supported`, the wrapper delegates to the inner `MmapDirectory::acquire_lock`,
and behaviour is bit-for-bit identical — that invariant is what task 3.0 must
demonstrate rather than assume.

- [x] 1.1 Replace `MmapDirectory::open` with `LenientLockMmapDirectory::open` at
  `searcher.rs:156`, `indexer.rs:34`, `indexer.rs:764`, `indexer.rs:814`. The six
  index builders inherit it through `open_or_create_index`. There is **one**
  test helper still using a bare `MmapDirectory`, at `indexer.rs:913` — leave it
  alone unless a test asserts on the directory type. (Verified 2026-08-25: those
  are the only four non-test `MmapDirectory::open` sites in `indexer.rs`.)
- [x] 1.2 In `open_single_index` **only**, use `Index::open` when the directory
  already contains an index, falling back to `Index::open_or_create` only when it
  does not (FR-11). Creating an index from the *search* path is never correct.
  `open_or_create` **stays** in every `indexer.rs` write path.
- [x] 1.3 Build the reader with `ReloadPolicy::Manual` (FR-17). Record in a
  comment that this is an independent improvement, **not** part of the lock fix
  (FR-18): `open_segment_readers` takes `META_LOCK` regardless of policy. The
  default spawns one 500 ms `meta.json`-polling thread **per index**
  (`directory/file_watcher.rs:12,47-62`) — six threads and ~12 reads/s against
  the user's FUSE volume today.
- [x] 1.4 Confirm nothing else depends on the reader auto-reloading: every index
  mutation is already followed by an explicit `reinit_fulltext_searcher()`. Grep
  the rebuild, import and reconcile paths and list them in the commit message.

  **The premise was not quite true, and this found the exception.** There are
  five in-app index-mutation sites (the `cli/` builders run in a separate
  process with no live searcher, so they do not count):

  | Site | Mutation | Reinit before this task? |
  |---|---|---|
  | `backend/src/lib.rs:447` `reconcile_dict_indexes_blocking_c()` | dict index reconcile | yes |
  | `bridges/src/dictionary_manager.rs` `start_reconcile()` | dict index reconcile | **no** |
  | `bridges/src/sutta_bridge.rs:4125` | library book import | yes |
  | `bridges/src/sutta_bridge.rs:4224` | `rebuild_search_index` | yes |
  | `bridges/src/sutta_bridge.rs:4446` | library language change | yes |

  `DictionaryManager::start_reconcile` is the **same reconcile** as the `lib.rs`
  one, reached from the GUI rather than from startup, and it never reinitialised
  the searcher — it was silently relying on the default reader's 500 ms
  `meta.json` poll, which is exactly what 1.3 removes. Left alone, a
  GUI-triggered reconcile would have gone unnoticed by the open searcher for the
  rest of the session. Fixed here by adding the `reinit_fulltext_searcher()`
  call, with a comment saying it is required rather than defensive.

  This is worth stating plainly in the commit message: **1.3 turned a latent
  staleness window into a hard dependency, and one call site had to be fixed to
  meet it.**
- [x] 1.5 Verify FR-16 holds in the shipped wrapper — once a directory is
  classified `Unsupported`, the inner `acquire_lock` is **skipped**, not called
  and discarded. Skipping avoids a failing syscall per reader reload and avoids
  creating a `.tantivy-meta.lock` that can never work (`MmapDirectory` creates
  lock files and never deletes them). If the shipped code does not do this, fix
  it here.

  **It already does** — `acquire_lock` calls `flock_support_for_dir(&self.root)`
  first and returns straight into `acquire_fallback_lock` on `Unsupported`,
  never reaching `self.inner.acquire_lock`. No change was needed. The behaviour
  is now **pinned by a test** rather than only by reading (see 1.6): the
  observable consequence of a skipped inner call is that no `.tantivy-meta.lock`
  file appears, since `MmapDirectory::acquire_lock` creates that file before
  locking it and never removes it.
- [x] 1.6 `cd backend && cargo test`. Add a unit test that exercises the fallback
  against a simulated unsupported-lock inner directory (fix-PRD success metric 8)
  if one does not already exist from phase 1.

  Phase 1's tests covered the fallback lock's *mechanics* but never the
  wrapper's `acquire_lock` **taking** the fallback route — on a developer
  machine the probe always answers `Supported`. Three tests added to
  `backend/src/search/lenient_directory.rs`, sharing a
  `pretend_flock_is_unsupported()` helper that seeds the module-private support
  cache (which is why these are unit tests, not integration tests):

  - `an_unsupported_volume_falls_back_without_touching_the_inner_lock` — the
    fallback produces a working lock, the route is recorded as
    `FallbackUnsupported`, **no lock file is created** (FR-16), and the guard is
    a real mutual exclusion that releases on drop (FR-13).
  - `an_unsupported_volume_still_refuses_a_second_writer` — FR-14: the
    non-blocking `INDEX_WRITER_LOCK` still returns `LockBusy` on contention, so
    the single-writer guarantee holds in-process.
  - `the_flock_probe_runs_at_most_once_per_directory` — FR-7.

  **`flock_probe_count()` / `flock_probe_count_for_dir()` were added** as
  always-compiled accessors over an `AtomicUsize` plus a per-directory map. This
  is task **3.4's option (a)**, landed early because 1.6 needed it too; the
  per-directory accessor is what makes the assertion immune to `cargo test`'s
  in-process parallelism, so 3.4 should use it rather than a global delta.

  `cargo test` result: **all 14 `lenient_directory` tests pass**, and the wider
  suite is green apart from `diacritic_query_highlights_bold_definition_rows`,
  which is a **timing-budget assertion that fails only under parallel load** and
  passes in isolation — the project's known absolute-time budget drift, not a
  regression from this task. That run also logged the thing this whole task
  exists for: `FulltextSearcher opened: 3 sutta language indexes, 2 dict
  language indexes, 1 library language indexes`.

### 2.0 [x] Honest readiness reporting (fix-PRD FR-19…FR-30)

**Specs to keep in mind.** The failure list and its clear-on-reopen rule already
exist (§2 above) — this task **consumes** them. The user's own report is the
argument: section F said 0/0/0 with 6 failures while section E opened all six,
and the app called itself "initialized" throughout.

- [x] 2.1 Expose the per-area open counts from `FulltextSearcher` (sutta / dict /
  library) through a `backend/src/lib.rs` accessor, so QML, `/health` and
  Database Validation read **one** source rather than re-probing.
- [x] 2.2 `reinit_fulltext_searcher()` (`lib.rs:344`): log at **ERROR** when it
  completes with zero indexes open, and include the counts in the message
  (FR-21). A log alone must tell the story.
- [x] 2.3 Make the distinction FR-30 requires: index directory **absent** →
  "Fulltext index files not found"; present but zero opened → the lock/IO error.
  Do not conflate them. This is the `StartupDbReport` principle.
- [x] 2.4 `is_fulltext_searcher_ready()` (`lib.rs:408`) must return **false**
  when zero indexes are open (FR-20). **This changes `/health`'s
  `fulltext_searcher_ready`** — deliberately, and it is why phase 1 deferred it
  (diagnostics FR-31b). Update
  `docs/simsapa-localhost-api-search-endpoints.md` in the same commit, and add
  the per-area counts to `/health` while there.
- [x] 2.5 Add a **"Fulltext index"** row to `DatabaseValidationDialog.qml`,
  driven by the existing `database_validation_result` signal — no new signal
  plumbing (FR-27). Emit it from `bridges/src/sutta_bridge.rs` beside the
  dictionaries emission at `:2247`. Valid when ≥ 1 index per expected area
  opened, reporting counts ("3 sutta, 2 dictionary, 1 library index"); invalid
  when the directory exists and zero opened, with the underlying error in plain
  language.
- [x] 2.6 Add the "index could not be opened" empty state to
  `FulltextResults.qml:335-350`, extending the existing non-generic branch rather
  than adding a dialog or toast (FR-23…FR-25). It must name the reason from the
  recorded failure and point at Database Validation. **It must not fire** when a
  language index is simply absent because the user never downloaded that language
  (FR-26) — gate on "a failure was recorded for an area being searched", not on
  "zero results".

  **This needs data QML does not have yet.** `searcher_open_failures()` lives in
  Rust; add a `SuttaBridge` invokable returning the recorded failures (JSON, the
  project's convention for structured bridge results) plus a matching stub in
  `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`. Task 2.1's per-area
  counts accessor should feed the same call, so the empty state, the Validation
  row and `/health` all read one source.
- [x] 2.7 Wording check: no `flock`, `ENOSYS`, `Tantivy`, `META_LOCK`, `mmap` or
  `FUSE` in any user-facing string. The user-facing concept is *"this storage
  location does not support the file locking the search index needs; Simsapa is
  working around it"*, and in the failure case *"the search index could not be
  opened."*
- [x] 2.8 `backend/src/storage_probe.rs`: record the `flock` and `mmap` verdicts
  (FR-31…FR-33) and log them next to the existing `storage_path` diagnostic. **A
  volume that fails only the `flock` test must not be demoted** (FR-32) — with
  task 1.0 in place the app works on it. The demote-only contract is unchanged.

  **The two verdicts ended up in two places, deliberately.**
  `probe_storage_location()` (tier 2, dialog-only) now runs the `flock` probe
  and logs it as *"recorded only, never demotes"*, leaving its `Result`
  untouched. It does **not** run the `mmap` probe: that probe is only meaningful
  against a file large enough to fault past page 0, and this probe's directory
  is a storage root the user is still choosing — writing an 18 MB file to a card
  to earn one log line is not a reasonable price.

  The `mmap` verdict is instead logged against the **real index directory**, by
  a new `storage_diagnostics::log_storage_capability_verdicts()` called from
  `reinit_fulltext_searcher()` — one line carrying **both** verdicts, once per
  process, on every platform (FR-33, success metric 10). That is also the
  honest place for it: it measures the directory the searcher is about to use.

**Fixture regenerated: `backend/tests/data/fulltext_search_so_ce_evam_vadeyya.json`.**
`test_fulltext_search_so_ce_evam_vadeyya` failed with `total` 2330 against an
expected 2331. This is **not** the project's known timing-budget drift — it is a
hit count — so it was measured rather than assumed:

- A throwaway A/B ran **both** open sequences in one process against the same
  on-disk `suttas/pli` index — old (`MmapDirectory` + `Index::open_or_create` +
  default reader) versus new (`LenientLockMmapDirectory` + `Index::open` +
  `ReloadPolicy::Manual`). Result: `num_docs=10649 hits=4304` for **both**.
  Task 1.0's change is byte-identical on this query.
- The regenerated fixture returns the **same 10 uids in the same order**; what
  moved is `total` (−1, 0.04%), the BM25 scores (~0.003%), and **two snippets**.
  A changed snippet for an unchanged uid means the underlying *sutta text*
  changed, not just the corpus size — i.e. `appdata.sqlite3` and the index have
  drifted from the fixture, which was last committed 2026-07-31.

So the fixture was stale, and regenerating it is the correct maintenance action.
Worth knowing: **this fixture is machine-dependent** — it is generated from
whatever index the dev machine currently holds, so it will drift again after any
re-bootstrap. The generator is
`cargo test --test test_fulltext_search_results -- --ignored generate_fulltext_fixture`.

### 3.0 [x] Benchmark: prove the wrapper costs nothing on a normal filesystem

**Specs to keep in mind.** Fix-PRD success metric 7 asks for "no change in
index-open time or search latency on desktop Linux". There is **no bench
infrastructure** in the repo, and the project has known drift in absolute-time
budgets — so this must be a **ratio between two directories measured in the same
run**, never a millisecond budget. The fast path's only theoretical costs are:
one `flock` probe per directory (cached for the process by
`flock_support_for_dir`), one enum comparison per `acquire_lock`, and a
`lock_paths` record push.

- [x] 3.1 **Take the baseline first, before task 1.0 lands.** Record it in this
  file so a later session can compare: index open, reader build, and search
  latency through a bare `MmapDirectory` on the real dev index
  (`/home/gambhiro/prods/apps/simsapa-ng-project/bootstrap-assets-resources/dist/simsapa/app-assets/index/`).

  **Baseline measured 2026-08-25**, before any of task 1.0 landed. Bare
  `MmapDirectory::open` → `Index::open` → `register_tokenizers` → `index.reader()`
  (default reload policy) → `Count` search on `content`, run from a throwaway
  integration test in `backend/tests/`, `--release`, `--test-threads=1`,
  6 iterations per index with the **first discarded** (page cache warm-up).
  `open_ms` covers `MmapDirectory::open` + `Index::open` + tokenizer
  registration; the search columns are microseconds.

  | index | num_docs | open_ms | reader_ms | nirodha_µs | cessation_µs |
  |---|---:|---:|---:|---:|---:|
  | suttas/en | 10649 | 0.06 | 0.61 | 68.6 | 14.7 |
  | suttas/hu | 494 | 0.06 | 0.61 | 54.8 | 4.0 |
  | suttas/pli | 10649 | 0.06 | 0.81 | 125.8 | 19.0 |
  | dict_words/en | 13587 | 0.05 | 0.17 | 24.1 | 10.1 |
  | dict_words/pli | 539569 | 0.06 | 0.70 | 142.8 | 49.6 |
  | library/en | 322 | 0.05 | 0.61 | 67.1 | 8.4 |

  Two things this run establishes, beyond the numbers:

  - **`Index::open` works on every dev index** — none of the six needed
    `open_or_create`, so task 1.2's read-path change has no fallback case here.
  - **The dev tree is not the user's** (§0.0): it has `suttas/hu` and no
    `suttas/san`, and `suttas/en` reports 10649 docs against the user's 10722.
    The benchmark of 3.2 must enumerate what it finds, as planned.

  Scale note for 3.3's ratio assertion: open and reader build are **sub-millisecond**
  here, so their ratios will be dominated by noise; the search columns are the
  stable signal. Averaging over enough iterations (and reporting all three
  unconditionally) is what makes the assertion diagnosable.
- [x] 3.2 Write `backend/tests/test_lenient_directory_benchmark.rs`. For each
  available index directory, run the identical sequence through
  `MmapDirectory` and through `LenientLockMmapDirectory`: open → `Index::open` →
  `reader()` → N searches (use the diagnostic's own terms, `nirodha` and
  `cessation`, so the numbers are comparable with the user's section E). Report
  both, and the ratio.

  Written as two tests in one binary. `discover_indexes()` walks
  `<SIMSAPA_DIR>/app-assets/index/<area>/<lang>` and keeps only directories
  where `Index::exists` is true — enumerated, never hard-coded, per 3.1's
  finding that the dev tree is not the user's. `run_once()` is the measured
  sequence, parameterised **only** by which `Directory` opens the index; both
  arms use `ReloadPolicy::Manual`, so the isolated variable really is the
  directory (and so the benchmark does not spawn watcher threads that would
  poison 3.6 in the same process).
- [x] 3.3 Assert on the **ratio**, generously — e.g. wrapper ≤ 1.25× bare for
  search latency and reader build, averaged over enough iterations to be stable.
  Discard the first iteration of each (page cache, and the one-time probe).
  Print the numbers unconditionally so a failure is diagnosable from the output.

  6 iterations per index per arm, first discarded. The 1.25× limit is applied to
  the **aggregate across all indexes**, not per index: open and reader build are
  sub-millisecond per index (3.1's scale note), so a single index's ratio is
  scheduling noise while the sum is stable. The full per-index table plus the
  aggregate line print before any assertion runs.
- [x] 3.4 Assert the probe runs **at most once per directory** (FR-7). This is
  the one cost that could scale with query volume if the cache broke.

  Uses option (a), `flock_probe_count_for_dir()`, which task 1.6 already landed
  for this reason. Per directory, never the process-global counter.

  **PITFALL — `#[cfg(test)]` will not work here.** A test in `backend/tests/` is
  an *integration* test: it links the library compiled **without** `cfg(test)`,
  so a `#[cfg(test)]` counter inside `lenient_directory.rs` is invisible to it.
  Either (a) make the counter an always-compiled `AtomicUsize` with a `pub fn
  flock_probe_count()` accessor — cheap, and useful in the field too — or
  (b) put this particular assertion in a `#[cfg(test)] mod tests` **inside**
  `backend/src/search/lenient_directory.rs` and leave only the timing comparison
  in the integration test. (a) is preferred; it is one relaxed atomic increment
  on a path that already does a syscall.
- [x] 3.5 Skip cleanly with a clear message when the dev index is absent, so the
  test is not a failure on a machine without it. It must never *create* an index.

  `index_root()` resolves `SIMSAPA_DIR` from the project `.env` **cwd-relative**
  (which is what the dev value is; `get_create_simsapa_dir()` makes the same
  fallback) and returns `None` rather than creating anything. Both tests print a
  `SKIPPED:` line naming what is missing and return. Every open in the file is
  `Index::open`, never `open_or_create`.
- [x] 3.6 Verify FR-17's win separately: assert that no
  `thread-tantivy-meta-file-watcher` threads exist after opening N indexes with
  `ReloadPolicy::Manual` (success metric 9). On Linux, read
  `/proc/self/task/*/comm` and match the thread **name**.

  `no_meta_file_watcher_threads_with_manual_reload`, `#[cfg(target_os = "linux")]`.
  Two details worth keeping: the readers are **held in a `Vec` for the duration
  of the scan** — a dropped reader takes its watcher thread with it, which would
  make the test pass for the wrong reason — and the match is on the prefix
  `thread-tantivy-`, because Linux truncates `comm` to 15 bytes so the full
  name never appears there.

  **PITFALL — cargo runs tests in parallel in one process.** Another test that
  builds a default-policy reader will spawn watcher threads into the *same*
  process and fail this assertion for the wrong reason. Match on the specific
  thread name (never a bare count), and note in the test's doc comment that it
  should be run with `--test-threads=1` if it ever proves flaky. The variant name
  is `ReloadPolicy::Manual` — verified against
  `tantivy-0.25.0/src/reader/mod.rs:21-31`, where the only other variant is
  `OnCommitWithDelay`.
- [x] 3.7 Record the measured before/after numbers **in this file** under task
  3.1, and in `docs/fulltext-index-storage-and-file-locking.md`. A benchmark
  whose results live only in a terminal that has scrolled away has not been run.

  **Measured 2026-08-26**, `--release --test-threads=1`, 6 iterations per index
  per arm with the first discarded. `_b` = bare `MmapDirectory`, `_w` = wrapper.

  | index | num_docs | open_b_ms | open_w_ms | read_b_ms | read_w_ms | srch_b_ms | srch_w_ms | open× | read× | srch× |
  |---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
  | dict_words/en | 13587 | 0.043 | 0.040 | 0.107 | 0.105 | 0.035 | 0.033 | 0.95 | 0.99 | 0.95 |
  | dict_words/pli | 539569 | 0.050 | 0.050 | 0.412 | 0.431 | 0.187 | 0.186 | 1.01 | 1.05 | 0.99 |
  | library/en | 322 | 0.049 | 0.047 | 0.384 | 0.397 | 0.085 | 0.079 | 0.97 | 1.03 | 0.93 |
  | suttas/en | 10649 | 0.048 | 0.047 | 0.323 | 0.321 | 0.081 | 0.077 | 0.99 | 0.99 | 0.94 |
  | suttas/hu | 494 | 0.052 | 0.048 | 0.360 | 0.393 | 0.057 | 0.056 | 0.93 | 1.09 | 0.99 |
  | suttas/pli | 10649 | 0.051 | 0.057 | 0.475 | 0.462 | 0.138 | 0.138 | 1.13 | 0.97 | 1.00 |

  Aggregate: open 0.292 → 0.291 ms (**1.00×**), reader 2.060 → 2.110 ms
  (**1.02×**), search 0.583 → 0.568 ms (**0.97×**). Ratios fall on both sides of
  1.0, which is what a no-cost result looks like. The debug profile reproduces
  the same conclusion (0.97× / 1.00× / 0.97×), so this is not an artefact of the
  optimisation level. **Fix-PRD success metric 7 is met.**

  One caveat when comparing against 3.1's baseline table: that baseline used the
  **default** reload policy and so also paid for spawning a watcher thread per
  index (0.61–0.81 ms reader build there vs 0.32–0.48 ms here). The two arms
  above are `ReloadPolicy::Manual` on both sides, which is the correct
  comparison for isolating the directory wrapper — the policy change is measured
  by 3.6 instead.

  Written up in the new `docs/fulltext-index-storage-and-file-locking.md`, whose
  mechanism sections task 8.2 still has to fill in.

### 4.0 [x] Get the dictionary import off the UI thread, with real progress

**Specs to keep in mind.** The import stage is **already** threaded with progress
and abort (§2) — do not rebuild it. The single blocker is the staging copy:
`copy_content_uri_to_temp` is synchronous on the GUI thread and does
`readAll()` of the whole archive (`cpp/utils.cpp:678`). At 180 MB that is
seconds of frozen UI and an ANR risk on a slower stream. Picker-URL PRD
Reqs. 10, 17b–17d.

- [x] 4.1 Add an **asynchronous** staging invokable on `DictionaryManager` —
  `stage_picked_file(url)` — that spawns a worker and reports through new
  signals `stagingProgress(done_bytes, total_bytes)`, `stagingFinished(path)`,
  `stagingFailed(message)`. Follow the existing `scan_source` shape
  (`bridges/src/dictionary_manager.rs:451-480`) exactly, including
  `crate::queue_or_log()`.

  Three mechanical details, each of which fails at runtime rather than at build
  time if missed:
  - **Signals need an explicit `#[cxx_name = "stagingProgress"]`.** cxx-qt does
    *not* auto-camelCase here — every existing signal in that file carries one
    (`:130-152`), and QML's `onStagingProgress` will simply never fire without it.
  - **Take the picked file as `&QUrl`, not `&QString`.** `DictionaryManager`
    currently has no `QUrl` in its bridge, so add the `cxx_qt_lib` import; this
    is the whole point of Req. 2, and `SuttaBridge::save_file` is the working
    precedent. Recover the string with `to_encoded()`, never `toString()`
    (`docs/android-file-saving-saf.md`).
  - **Add stubs for the invokable *and all three signals*** to
    `assets/qml/com/profoundlabs/simsapa/DictionaryManager.qml`, or `qmllint`
    fails — CLAUDE.md. `SuttaBridge.qml:45-48` is the shape to copy.

  All three details were followed as written. Two things worth recording beyond
  them:

  - **The copy itself is now Rust, not C++.** `backend/src/import_staging.rs` is
    new and holds the whole platform-independent half (request shape, free-space
    check, name sanitizing, the chunked writer, the typed error); the Android
    half is `android_saf::copy_document_to_path` + `document_metadata`, next to
    the JNI stack that already exists there. That also **delivers task 6.4
    early**: the provider read goes through
    `ContentResolver.openInputStream`, never `QFile(content_uri)`, because
    writing the new copy in C++ around `QFile` would have meant writing the
    thing 6.4 exists to remove.
  - **`stagingProgress` carries `f64`, not `i32`.** A byte count is not
    guaranteed to fit in an `i32` (a 3 GB archive is legal), and QML numbers are
    doubles anyway, so `i32` would have been a silent wrap at the one size where
    progress matters most.
- [x] 4.2 Copy in **fixed-size chunks** (1 MB), not `readAll()` (Req. 10), and
  emit `stagingProgress` per chunk. Distinguish "read produced zero bytes" from
  "read succeeded" and report the former as a failure (Req. 11). Every failure
  branch must name **which** step failed — URI parse, resolver open, temp dir
  creation, write, short write (Req. 12).

  `CHUNK_BYTES = 1 MB` on both the desktop and the Android reader. Progress is
  emitted **throttled to 100 ms**, not literally per chunk: a 1 MB chunk of a
  local copy completes in well under a millisecond, and a queued cross-thread
  signal per chunk would cost more than the copy. Nothing is lost —
  `stagingFinished` carries the terminal state.

  `StagingError` carries a stable `code` plus a `step` and a `message`, and
  `user_message()` is `"<step>: <message>"`, so an unattributed staging failure
  is now unrepresentable. Codes: `no_file`, `not_found`, `unsupported_scheme`,
  `insufficient_space`, `staging_dir`, `provider_open_failed`,
  `provider_read_failed`, `write_failed`, `short_write`, `empty_read`,
  `cancelled`. A zero-byte read is `empty_read` **and the empty file is
  deleted** — leaving it would have been a path to a 0-byte "archive" that
  fails much later as corrupt. Every failure path removes the partial copy.
- [x] 4.3 Check free space on the staging volume before starting and fail with a
  clear `insufficient_space` message (Req. 24). `fs4` is already a direct
  `backend` dependency. Measure the **staging volume actually used**, not `C:`
  or the app root.

  `ensure_free_space()` measures the **per-feature staging folder**
  (`<temp>/simsapa-imports/dictionaries/`), walking up to the nearest existing
  ancestor because `statvfs` needs a real path and the folder may not exist yet
  — same volume either way. Requires the file plus a 32 MB margin. Two
  deliberate non-failures: an **unknown** size (a provider that reports no
  `_size`) skips the check, since an unmeasurable file is not evidence of a full
  disk; and an unreadable `statvfs` logs and continues rather than refusing an
  import over a missing figure.
- [x] 4.4 Add a **"Copying file…"** state to `DictionaryImportDialog`'s
  `StackLayout`, before the existing "Scanning…" frame (Req. 17b), determinate
  where the byte count is known. Reuse the visual language of the scanning frame
  (`:375-410`); do not invent a new window — `DictionaryIndexProgressWindow.qml`
  is the pattern to imitate if a separate window is preferred.

  Added as a frame in the existing `StackLayout`, reusing the scanning frame's
  visual language — no new window. The bar is determinate when the provider
  declared a size and indeterminate when it did not, with a byte label under it
  either way (`"12.0 MB of 172.4 MB"` / `"12.0 MB copied"`), so **0 % never
  stands in for "unknown"**.

  The four frame indices are now **named `readonly property int`s** on `root`
  (`frame_source` / `frame_copying` / `frame_scanning` / `frame_checklist`).
  Inserting a frame renumbered every literal in the file; naming them is what
  makes that safe to do again.
- [x] 4.5 Give the scanning frame a **cancel** affordance and a stage label, so a
  177 MB extraction is not an indeterminate bar with no text.

  **Two different cancels, because only one of them can be real today.**

  - The **copying** frame's Cancel is a true cancel: `abort_staging()` sets an
    `AtomicBool` on `DictionaryManagerRust` that the copy checks between chunks;
    the worker deletes the partial file and reports `cancelled`. It travels the
    same `stagingFailed` channel as a real failure (one outcome path, not two)
    and the dialog tells them apart with a `staging_cancelled` flag it set
    itself — never by matching the message text.
  - The **scanning** frame's Cancel *abandons* rather than cancels:
    `scan_source` has no cancel flag in the backend, so the worker runs to
    completion and its result is discarded. That is said plainly in the code
    comment rather than dressed up. Task **5.1** removes the expensive half of a
    scan (a full archive extraction) and **5.3** makes the import's extraction
    cancellable, which is what would make a real cancel here worth adding.
- [x] 4.6 **Bracket the whole flow with `AssetManager.set_keep_screen_on`** —
  currently called **nowhere** in the dictionary import flow, which is a standing
  CLAUDE.md violation and lets the device suspend mid-import. Use distinct holder
  names per independent holder, e.g. `dictionary-import-staging` and
  `dictionary-import-batch`. Release in the completion handlers on **both**
  success and failure, never in a dialog's `onClosed` — the worker outlives the
  dialog.

  **Three holders, not two** — the three stages are independently long and can
  end independently: `dictionary-import-staging` (acquired in `begin_staging`,
  released in **both** `onStagingFinished` and `onStagingFailed`),
  `dictionary-import-scan` (acquired in `begin_scan`, released in both
  `onScanFinished` and `onScanFailed` — including when the user abandoned the
  scan, since the hold must outlive the abandonment, not the dialog state), and
  `dictionary-import-batch` in `DictionariesWindow` (acquired in `start_batch`,
  released in `finish_batch`, which every ending goes through: success,
  per-item failure and abort alike). Neither window had an `AssetManager`
  before; both now have one, named `screen_manager` and used for nothing else.
- [x] 4.7 Leave `SuttaBridge.copy_content_uri_to_temp` in place for the other
  three call sites (document, chanting, Gloss); this task does not migrate them
  (§0.4). Add a comment on it naming the async replacement and why only the
  dictionary path uses it.

### 5.0 [ ] Extract once, and clean up what a cancel leaves behind

**Specs to keep in mind.** Today a 172 MB archive is fully extracted **twice** —
once by `probe_zip_candidate` to read the `.ifo`, once by `import_user_zip`
(Req. 23, Req. 25). `zip` 2.x can read individual entries by name, so the probe
never needs to extract at all.

- [ ] 5.1 Rewrite `probe_zip_candidate` (`dictionary_manager_core.rs:459`) so it
  stops extracting the whole archive (Req. 25).

  **PITFALL — the `stardict` crate cannot read from an archive.** `probe_stardict_dir`
  (`:448-454`) calls `Ifo::new(ifo_path)` and `stardict::no_cache(ifo_path)`, and
  **both take a filesystem `Path`** (`stardict = "0.2.2"`, `backend/Cargo.toml:29`).
  So "read it out of the zip" is not a drop-in change. Take it in this order:
  1. Enumerate entry names only (`ZipArchive::file_names()`) — no decompression.
     This alone answers task 7.0's format question and costs nothing.
  2. For the title and count, prefer parsing the **`.ifo` entry** directly: it is
     a small key=value text file and the StarDict spec requires a `wordcount`
     field, so reading that one entry may make the `.idx` unnecessary.
     `probe_stardict_dir` currently reports `dict.idx.items.len()` — decide
     explicitly whether the declared `wordcount` is acceptable for the checklist
     display, and say so in a comment. It is only shown to the user.
  3. **Only if** the `.idx` is genuinely needed, extract **just those entries**
     into a small temp dir and call the existing helpers on it. Verify first
     whether `stardict::no_cache` also requires the `.dict`/`.dict.dz` to be
     present — if it does, that is the bulk of the archive and step 2 is the only
     acceptable route.

  Whatever route is taken, the outcome must be: **no full extraction during a
  scan.**
- [ ] 5.2 Change the probe's return type from `Option<CandidateMeta>` to a
  **typed result** that distinguishes: a valid StarDict; a recognised
  non-StarDict format (see 7.0); an unreadable/corrupt archive; and an I/O or
  space failure. `scan_source` must propagate the reason instead of returning an
  empty vector (this is what task 7.0 renders).
- [ ] 5.3 Make `import_user_zip`'s extraction **cancellable**: `archive.extract()`
  is a single opaque call today and the `cancel: &AtomicBool` is only consulted
  inside `import_stardict_as_new`. Extract entry-by-entry, checking `cancel`
  between entries, and emit `StardictImportProgress::Extracting` with a count so
  the existing progress frame becomes determinate.
- [ ] 5.4 Verify Req. 30 against `zip` 2.x: confirm `extract()` (or the
  entry-by-entry replacement) rejects path-traversal entries — `../`, absolute
  paths — via `enclosed_name` or equivalent. **Verify, do not assume**; the
  extraction target is inside `SIMSAPA_DIR`. Add a unit test with a crafted
  archive.
- [ ] 5.5 Delete the staged `.zip` when the import completes **or is cancelled**
  (Req. 21a). Today nothing does: only `DocumentImportDialog` ever calls
  `delete_temp_import_folder`. Stage into a **per-feature subfolder**
  (`<TempLocation>/simsapa-imports/dictionaries/`, Req. 18) and delete only that
  (Req. 19) — the shared-root wipe is Defect D's claim 1, which the 08-25
  measurements did **not** retire. The staged file must survive from
  `scan_source` recording it as `source_path` until the import ends (Req. 21).
- [ ] 5.6 Add a **startup sweep** for orphaned `simsapa-stardict-*` and
  `simsapa-stardict-probe-*` directories in `SIMSAPA_DIR`. `TempDir` cleans up on
  drop, but a killed process leaves them forever and nothing reclaims them.
  Age-gate it (e.g. older than an hour) so a concurrent import is never swept.
  Log what it removes.
- [ ] 5.7 Req. 20 (`std::env::temp_dir()` vs `QStandardPaths::TempLocation`) is a
  **non-issue** — measured identical on an Android 16 phone and on ARC
  (`staging_roots_differ: no`). Do not "fix" it. Record that in the code comment
  at `delete_temp_import_folder` so it is not re-investigated.

### 6.0 [ ] Make the dictionary import actually work — the automatic picker fallback (E-4, E-7, E-14…E-17)

**Specs to keep in mind.** §0.3: Qt's `FileDialog` returns nothing on this
device while our raw intent works perfectly. We do not know which of the four
deltas is responsible, and the user should not have to wait for another round
trip to find out. So the import **tries Qt's dialog and falls back**, and the
fallback firing *is* the measurement.

- [ ] 6.1 Drop `nameFilters` from `DictionaryImportDialog.qml`'s `file_dialog`
  (`:171`) **on Android only** (E-4). Desktop keeps
  `["StarDict archives (*.zip)"]`. Single, clearly-marked, revertible edit,
  commented with what it tests and pointing at PRD §4A.7. Keep the `title` — with
  no filter the picker now lists every file, so the title is the only thing left
  saying what is wanted.

  Two details: gate on **`Qt.platform.os === "android"`, not `is_mobile`** — iOS
  has neither the defect nor the raw-picker fallback, and `AboutDialog.qml:104-107`
  already documents this exact distinction. And an **empty array** is the correct
  "no filter" value: Qt's `setMimeTypes()` tests `if (!nameFilters.isEmpty())`
  (`qandroidplatformfiledialoghelper.cpp:151`), so `[]` yields
  `setType("*/*")` and no `EXTRA_MIME_TYPES` — exactly the diagnostic's
  configuration.
- [ ] 6.2 Add the empty-URL guard (E-7): if `selectedFile` is empty or invalid,
  **never** call `scan_source` with `""`. This is what produced `Path not found: `
  with nothing after the colon. It must be distinct from the existing
  *"Could not access the selected file."*
- [ ] 6.3 On Android, when the guard fires, **automatically retry with the raw
  `ACTION_OPEN_DOCUMENT` picker** rather than giving up. Requirements:
  - tell the user first, in one sentence, that the chooser returned nothing and
    Simsapa will try a different one — a second picker appearing unannounced
    reads as a bug;
  - log which path was used and which succeeded, under a greppable prefix, at
    INFO. **This is the measurement**: the fallback firing tells us Qt's dialog
    failed, and the raw pick succeeding tells us the file was reachable all
    along;
  - `FILE_SELECTION_TEST_THREAD` (`bridges/src/sutta_bridge.rs:84`) is a
    **single global slot** shared with the About dialog's test button. Add a
    discriminator (an enum or a caller token) so a dictionary-import pick and a
    diagnostic pick cannot be delivered to the wrong listener. Do **not** add a
    second parallel mechanism.
  - **§11 Q0a's terms are being revisited deliberately, not by drift.** Phase 1
    confined the private-Qt include (`QtCore/private/qandroidextras_p.h`,
    `Qt6::CorePrivate`) to the diagnostic precisely so a shipping feature could
    not be taken down by a build break at the next Qt upgrade. Promoting it to
    the import path is a decision: record it in the PRD's §11 Q0a with the
    reason (the alternative is leaving the user unable to import at all), and
    keep the include in `cpp/android_raw_pick.cpp` alone so the blast radius is
    unchanged. The failure mode remains a **compile error at upgrade time**, on
    ~80 lines of `#ifdef`-gated code.
- [ ] 6.4 Switch the dictionary import's provider read from
  `QFile(content_uri)` (`cpp/utils.cpp:662`) to
  `ContentResolver.openInputStream` — Req. 8 / Defect C, whose implementation
  already exists as `probe_document_uri` in `backend/src/android_saf.rs`. If the
  picker moves, the reader must move with it; leaving `QFile` behind is a second
  untested delta on the same path. Apply the **`to_encoded()` rule** in both
  directions (`docs/android-file-saving-saf.md`).

  **Already delivered by task 4.1** — verify rather than re-implement. The
  dictionary path's provider read is now
  `android_saf::copy_document_to_path` (`ContentResolver.openInputStream`,
  chunked), reached from `import_staging::stage_provider_uri`, and the URI it
  receives is `QUrl::to_encoded()` from `stage_picked_file`. `QFile(content_uri)`
  survives in `cpp/utils.cpp` only for the three unmigrated call sites (4.7).
  It fell out of 4.1 because writing the new chunked copy in C++ around `QFile`
  would have meant building the exact thing this task removes.
- [ ] 6.5 Log a `DICTIONARY-IMPORT-PICK:` block on the real import path
  (E-14…E-17), through the **same** `PickerUrlFacts` pipeline as the diagnostic —
  never a second report shape. Log on the **failure** path too (E-15). Do **not**
  perform the diagnostic's 4 MB provider read (E-16): staging is about to read
  the file for real. Observation only; desktop behaviour byte-identical (E-17).
- [ ] 6.6 Fix `outcome_line()` (`backend/src/picker_url.rs:843`) so a success
  does not read as a failure (§0.3.7). It is currently a function of the *input*
  and cannot see whether the read worked; pass the result in. On Android every
  successful pick is `PickerBranch::Provider`, and the only cheerful arm
  (`LocalFile`) is unreachable there — so **no Android user can see a line that
  sounds like it went well.** Suggested: *"The file chooser worked. Simsapa
  opened «all-dictionaries-gd.zip» (172 MB) and read it successfully."* Leave the
  scheme in the log.
- [ ] 6.7 Verify no new Android permission and that
  `android/AndroidManifest.xml` is **byte-identical** (Req. 26). Qt's dialog and
  our intent both need nothing.

### 7.0 [ ] Say what a non-StarDict archive actually is (MDict piece 1 only)

**Specs to keep in mind.** The user had a valid StarDict file and an MDict file
and could not tell them apart (§0.3.6). **MDict *reading* is dropped** — this is
naming only. It depends on task 5.1's entry-list read, so no extraction is
needed to answer the question.

- [ ] 7.1 Recognise archive contents by entry name: **MDict** (`.mdx`, `.mdd`),
  **DSL** (`.dsl`, `.dsl.dz`), **XDXF** (`.xdxf`), and "a zip of something else".
  Report the format in the typed probe result from task 5.2.
- [ ] 7.2 Render it in `DictionaryImportDialog.qml` in place of the current bare
  *"No StarDict dictionaries were found in the chosen source."* (`:154`).
  Required wording properties:
  - name the format found: *"This is an MDict dictionary (`.mdx`), which Simsapa
    cannot read."*;
  - **name the format wanted, including the GoldenDict alias** — many users know
    it only by that name: *"Please select a StarDict dictionary — often
    distributed as a GoldenDict (`-gd`) archive."*;
  - keep it to two sentences (PRD §7): what failed, what to do next;
  - distinguish "unsupported format" from "this archive could not be opened" and
    from "there was not enough space" — three different failures that all read
    as "no dictionaries found" today.
- [ ] 7.3 `scan_source` must reject a string that still carries a URL scheme with
  a distinct message (Req. 15) — *"Expected a file path but received a URL: …"*.
  Match on `://` or a parsed scheme, **never on a bare `:`** (`C:/Users/…` is a
  Windows path, §9.6). Safety net; after task 6.0 it should be unreachable.
- [ ] 7.4 Unit-test the classifier with fixture archives: a real StarDict, an
  MDict, a DSL, an empty zip, and a corrupt zip. No extraction, no temp dirs.

### 8.0 [ ] Tests, docs, and the build to send

- [ ] 8.1 `cd backend && cargo test` and `make qml-test` pass. New unit tests
  from 1.6, 3.x, 5.4 and 7.4.
- [ ] 8.2 Write `docs/fulltext-index-storage-and-file-locking.md` (fix-PRD §7):
  the `flock`-vs-`fcntl` distinction and why SQLite and the storage probe pass
  while Tantivy fails; the two Tantivy lock sites and their differing failure
  mappings (`reader/mod.rs:194` blocking `META_LOCK` → `IoError`;
  `index/index.rs:545` non-blocking `INDEX_WRITER_LOCK` → `LockBusy`, errno
  swallowed); the single-process invariant the fallback depends on; the
  `mmap`-works measurement and why the non-mmap contingency was dropped; the
  benchmark numbers from 3.7. **Title and frame it around filesystems without
  `flock`, listing ChromeOS/ARCVM and portable SD cards as two instances of one
  class — not around "SD cards"** (fix-PRD §10.3). Cross-link from
  `docs/relocated-storage-recovery.md`, `docs/storage-diagnostics.md`,
  `docs/search-snippet-highlight-pipeline.md` and `CLAUDE.md`'s notable-docs list.
- [ ] 8.3 Write `docs/dictionary-import-pipeline.md`: pick → stage → probe →
  import, which stage runs on which thread, the signal surface, the
  single-extraction rule, the cleanup owners (per-feature subfolder, the startup
  sweep, `TempDir` on drop), the keep-screen-on holders, and the format-detection
  table. Record the `readAll()`-on-the-UI-thread defect as **fixed** so it is not
  reintroduced.
- [ ] 8.4 Update `docs/file-selection-test.md` (§5.3 already records the returned
  report) with whatever task 6.0 changes, and
  `docs/simsapa-localhost-api-search-endpoints.md` for the `/health` change from
  2.4. Update `PROJECT_MAP.md` and `CLAUDE.md` per the standing rule.
- [ ] 8.5 Update the three source PRDs' status headers: the fulltext fix from
  "unblocked" to implemented; the picker PRD's phase-1b section with what
  actually shipped and what the fallback logging will tell us.
- [ ] 8.6 Non-goal verification, by grep: `android/AndroidManifest.xml`
  byte-identical; the four call sites of picker-PRD §2.6 **unchanged except the
  dictionary one**; no `console.` in touched QML; no `qInfo`/`qWarning` added; no
  `.exists()` added; no second `ANALYZE` added; no `.unwrap()` on a
  `qt_thread.queue()`.
- [ ] 8.7 Build the beta package for the user (`make android-beta-dist` → the
  `io.github.simsapa.app.beta` package, which installs *alongside* their Play
  copy — see `docs/android-beta-distribution-and-play-policy.md`) and write the
  covering message.

  **What to ask them to do, in this order:**
  1. Import **`all-dictionaries-gd.zip`** — the one in
     `Documenti/Dizionari`. **Explicitly say: not the `mdict` one**, which
     cannot work and would produce an ambiguous result.
  2. Run a fulltext search — suggest a word they will recognise, and note that
     search worked before only for "contains" style matches.
  3. Send `log.txt` from **About → log file list → Copy Contents / Save As…**
     (the ordering rule from picker-PRD Appendix B.1 still applies: do the two
     actions *first*, copy the log *afterwards*).

  Both fixes are then confirmed or refuted in one round trip. What to read in
  the returned log: `DICTIONARY-IMPORT-PICK:` (which picker path was used, and
  whether the fallback fired), `FulltextSearcher opened:` (non-zero counts), and
  the absence of `Failed to acquire Lockfile`.

---

## 9. Open questions

1. **Does the `nameFilters` removal alone fix the picker?** The fallback of task
   6.3 makes the answer non-blocking — the import works either way — but the log
   will say, and the answer decides whether the private-Qt dependency can be
   removed again later.
2. **Should the fallback become the *primary* picker on Android?** Only if the
   log shows Qt's dialog failing even without the filter. Do not pre-empt it.
3. **Is `probe_document_uri` fast enough for a 180 MB read on ARC?** Measured at
   4 MB (6–15 ms); never at full size. Task 4.2's chunked copy with progress is
   what makes a slow answer survivable rather than a hang.
4. **Fulltext row in `StartupDbReport`** (fix-PRD §9.4) — still open. The
   reporting user found the failure only because we sent them a diagnostic
   button; a startup-report row would have surfaced it unprompted.
5. **§4.7's removable-volume notice** — deferred (§0.4). Revisit only if a
   genuinely slow affected volume is ever measured.
6. **Is the `.ifo`'s declared `wordcount` an acceptable substitute for
   `dict.idx.items.len()`** in the import checklist? (Task 5.1.) It decides
   whether a scan can avoid touching the `.idx` at all. The number is only ever
   shown to the user, so the bar is "is it honest", not "is it exact" — but if
   the two can disagree materially for real dictionaries, say which is shown.
7. **Does `stardict::no_cache` require the `.dict`/`.dict.dz` to be present?**
   Unverified. If it does, selectively extracting the `.idx` is not cheap after
   all and task 5.1 must take the `.ifo`-only route. Check before writing 5.1.
