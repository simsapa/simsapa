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

- `backend/src/dictionary_manager_core.rs` — **the bulk of 5.0.** New:
  `ArchiveFormat` / `detect_archive_format`, `ProbeOutcome` / `ScanRejection` /
  `ScanReport`, `read_ifo_title_and_count` (the `.ifo`-only probe),
  `is_shallow_ifo_entry`, `shallow_file_names`, `rejection_for`,
  `extract_archive` (entry-by-entry, cancellable, `enclosed_name`-guarded),
  `sweep_orphaned_extract_dirs`, and the `EXTRACT_TEMP_PREFIX` /
  `PROBE_TEMP_PREFIX` constants. Rewritten: `probe_zip_candidate` (no
  extraction), `probe_dir_candidate`, `scan_source` (returns `ScanReport`, and
  rejects a URL string). `probe_stardict_dir` and the `stardict::no_cache` call
  are **gone**. Its `#[cfg(test)] mod tests` carries the hand-built-zip
  traversal test (5.4).
- `backend/src/stardict_parse.rs` — `StardictImportProgress::Extracting` now
  carries `{ done, total }` (5.3); `cli/src/main.rs`,
  `cli/src/bootstrap/mod.rs` and `bridges/src/dictionary_manager.rs` updated.
- `backend/src/import_staging.rs` — **new (5.5):** `cleanup_staged_file`, which
  deletes a staged copy only when it is inside that feature's staging folder.
- `backend/src/lib.rs` — `init_app_data()` spawns the 5.6 sweep.
- `bridges/src/dictionary_manager.rs` — `cleanup_staged_file` invokable;
  `scan_source` serialises the `ScanReport` object; the empty-abort branch
  skips the delete for the `dictionary_id: -1` extraction cancel.
- `bridges/src/sutta_bridge.rs` — 5.7's comment on `delete_temp_import_folder`.
- `assets/qml/DictionaryImportDialog.qml` — `staged_path` +
  `discard_staged_file()` wired into every exit; `onScanFinished` reads the
  `ScanReport` shape and renders the rejection reasons.
- `assets/qml/DictionariesWindow.qml` — `finish_batch()` deletes the batch's
  staged copies.
- `assets/qml/com/profoundlabs/simsapa/DictionaryManager.qml` — stub for
  `cleanup_staged_file`.
- `backend/tests/test_dictionary_import_dir.rs`,
  `backend/tests/stardict_import_per_chunk_commit.rs` — updated for the new
  report shape and the new cancel ordering.
- `backend/src/picker_url.rs` — **the shared report pipeline (6.5, 6.6).** New:
  `PickReport` (prefix + read permission), `IMPORT_LOG_PREFIX`, the `Block`
  struct that makes a prefixless line inexpressible, `PickReportOutput`,
  `build_pick_report()`, `log_import_pick()`, `human_bytes()`, and the
  `filter_config` input field. `outcome_line` now takes the probe.
  `run_file_selection_test()` keeps its old signature as a wrapper.
- `bridges/src/sutta_bridge.rs` — **the fallback (6.3, 6.5).**
  `FILE_SELECTION_TEST_THREAD` became `RAW_PICK_TARGET`, carrying a
  `RawPickConsumer` discriminator alongside the thread handle;
  `on_raw_pick_finished()` dispatches; new `on_import_raw_pick_finished()`,
  the `log_import_pick` / `start_import_raw_pick` invokables, the
  `importFilePickCompleted` signal, and `RAW_INTENT_FILTER_CONFIG`.
- `bridges/src/dictionary_manager.rs` — **new `stage_picked_uri(&QString)`**
  (6.3), with the worker half shared through `spawn_staging(request)`.
- `assets/qml/DictionaryImportDialog.qml` — **6.1–6.3.** Android-only
  `nameFilters`, the `filter_config` property, `handle_picked_url()` with the
  empty-URL guard, `fallback_notice_dialog`, `start_fallback_pick()`,
  `begin_staging_uri()` / `enter_copying_frame()` / `finish_staging_start()`,
  and the `SuttaBridge` `onImportFilePickCompleted` handler.
- `cpp/android_raw_pick.cpp` — comment only: the two "diagnostic only, delete
  it" rules replaced by the revised §11 Q0a terms.
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

### 5.0 [x] Extract once, and clean up what a cancel leaves behind

**Specs to keep in mind.** Today a 172 MB archive is fully extracted **twice** —
once by `probe_zip_candidate` to read the `.ifo`, once by `import_user_zip`
(Req. 23, Req. 25). `zip` 2.x can read individual entries by name, so the probe
never needs to extract at all.

- [x] 5.1 Rewrite `probe_zip_candidate` (`dictionary_manager_core.rs:459`) so it
  stops extracting the whole archive (Req. 25).

  **Open question 7 is answered, and it decided the route: `stardict::no_cache`
  DOES require the `.dict`/`.dict.dz`.** `stardict-0.2.3/src/lib.rs:163-164`
  calls `get_sub_file(prefix, "dict", "dz")` and returns `Error::NoFileFound`
  when neither is present — that is the bulk of the archive, so step 3
  (selectively extracting the `.idx`) was never cheap. The `.ifo`-only route of
  step 2 is the only acceptable one, exactly as the task anticipated.

  So the probe now reads **two things and nothing else**: the central directory
  (`ZipArchive::file_names()` — entry names, zero decompression) and the single
  `.ifo` entry, a few hundred bytes of `key=value` text written to a small temp
  folder because the `stardict` crate parses from a filesystem path only. That
  temp folder holds one text file, not the archive.

  **Open question 6 is answered too, and recorded in a comment on
  `read_ifo_title_and_count`: the declared `wordcount` is what is shown.** It is
  required by the StarDict spec, it is only ever *displayed* in the checklist
  (the import counts what it actually inserts), and the alternative costs a full
  extraction. `probe_dir_candidate` was moved onto the same `.ifo` read for the
  same reason and for a second one: a dictionary and its own extracted folder
  now report the **same** number, which they did not before.
- [x] 5.2 Change the probe's return type from `Option<CandidateMeta>` to a
  **typed result** that distinguishes: a valid StarDict; a recognised
  non-StarDict format (see 7.0); an unreadable/corrupt archive; and an I/O or
  space failure. `scan_source` must propagate the reason instead of returning an
  empty vector (this is what task 7.0 renders).

  `ProbeOutcome` has those four variants. `scan_source` now returns a
  `ScanReport { candidates, rejections }` — **not** an enum, because a folder
  scan legitimately produces both at once (three StarDict archives and one
  MDict). Each `ScanRejection` carries a stable `reason`
  (`unsupported_format` / `unreadable` / `io_failure`), an optional `format`,
  and one plain sentence. The bridge serialises the report object, so
  `scanFinished` now carries `{"candidates":[…],"rejections":[…]}` rather than a
  bare array; `DictionaryImportDialog` reads the new shape and falls back to the
  old sentence only when there is genuinely nothing to say.

  **Two things landed here that belong to task 7 and are flagged rather than
  claimed.** `ArchiveFormat` + `detect_archive_format()` (7.1) exist because
  5.2's "recognised non-StarDict format" variant is not representable without
  them, and 5.1's entry-name read is the input. And **Req. 15 / task 7.3** — the
  URL-scheme rejection — was added to `scan_source` while its failure surface
  was being made typed: `://`, never a bare `:` (§9.6). Both are unit-tested
  here. Task 7.2 (the dialog wording) and 7.4 (fixture archives) are untouched,
  and 7.0 stays open.
- [x] 5.3 Make `import_user_zip`'s extraction **cancellable**: `archive.extract()`
  is a single opaque call today and the `cancel: &AtomicBool` is only consulted
  inside `import_stardict_as_new`. Extract entry-by-entry, checking `cancel`
  between entries, and emit `StardictImportProgress::Extracting` with a count so
  the existing progress frame becomes determinate.

  `extract_archive()` replaces `ZipArchive::extract`. `StardictImportProgress::
  Extracting` gained `{ done, total }`; the single pre-open tick still carries
  `0, 0`, which QML already renders as indeterminate.

  **This changed one observable behaviour, and it broke a test that was right to
  break.** A cancel now fires *before* any `dictionaries` row exists, so there
  is no 0-entry row to clean up — the importer reports `dictionary_id: -1` and
  the bridge's empty-abort branch skips the delete instead of asking to remove a
  row that was never created. `empty_abort_removes_zero_entry_row` asserted the
  old ordering; it was moved onto `import_user_dir` (no extraction stage), where
  its actual subject — the between-chunk insert cancel — still lives, and a new
  `cancelling_during_extraction_creates_no_dictionary_row` covers the new case
  by asserting the dictionary count is unchanged.
- [x] 5.4 Verify Req. 30 against `zip` 2.x: confirm `extract()` (or the
  entry-by-entry replacement) rejects path-traversal entries — `../`, absolute
  paths — via `enclosed_name` or equivalent. **Verify, do not assume**; the
  extraction target is inside `SIMSAPA_DIR`. Add a unit test with a crafted
  archive.

  `extract_archive` routes every entry through `ZipFile::enclosed_name()` (public
  in 2.4.2) and skips + logs anything it refuses. Symlink entries are written as
  ordinary files rather than recreated — strictly the safer of the two, and a
  StarDict archive has no symlinks to honour.

  **"Verify, do not assume" nearly failed on the test itself.** The first
  version built the crafted archive with `ZipWriter::start_file`, which
  **normalizes the name** (`options.normalize()`, `zip-2.4.2/src/write.rs:1172`)
  — `../escaped.txt` is stored as `escaped.txt`, so the test passed without ever
  testing traversal. It only came to light because an assertion that the archive
  really contained a `..` entry was added to check exactly that, and failed. The
  test now writes the local headers, central directory and EOCD **by hand**
  (`zip_with_raw_names`, with a 12-line CRC-32) so the hostile name reaches the
  central directory verbatim, and keeps that assertion as the guard against the
  test going vacuous again.
- [x] 5.5 Delete the staged `.zip` when the import completes **or is cancelled**
  (Req. 21a). Today nothing does: only `DocumentImportDialog` ever calls
  `delete_temp_import_folder`. Stage into a **per-feature subfolder**
  (`<TempLocation>/simsapa-imports/dictionaries/`, Req. 18) and delete only that
  (Req. 19) — the shared-root wipe is Defect D's claim 1, which the 08-25
  measurements did **not** retire. The staged file must survive from
  `scan_source` recording it as `source_path` until the import ends (Req. 21).

  The per-feature subfolder was already task 4.3's `staging_dir(feature)`; what
  was missing was the delete. `import_staging::cleanup_staged_file(path,
  feature)` removes one file and **decides ownership by location, not by the
  caller's word** — a path outside `simsapa-imports/dictionaries/` is refused,
  so a desktop pick (the user's own archive, `was_copied: false`) can never be
  deleted whatever QML passes in.

  **Req. 19's literal wording — change `delete_temp_import_folder`'s signature to
  take the feature — was deliberately not followed.** That function is reached
  only from `DocumentImportDialog`, which still stages through the C++ writer
  into the **shared** root; giving it a `"documents"` argument would point it at
  a subfolder nothing writes to and silently turn the one cleanup that does
  exist into a no-op. The requirement's *intent* (never wipe the shared root out
  from under another feature) is met by the dictionary path not using it at all.
  Re-shaping the other three call sites is the shared-resolver work §0.4 keeps
  out of this build.

  Ownership is explicit at every exit: the dialog holds `staged_path` and
  discards it on Cancel from either frame, on an abandoned scan, on a scan that
  found nothing, and on re-entry to `start()`; on Import it **clears**
  `staged_path`, handing ownership to `DictionariesWindow.finish_batch()`, which
  every ending goes through — success, per-item failure and abort alike.
- [x] 5.6 Add a **startup sweep** for orphaned `simsapa-stardict-*` and
  `simsapa-stardict-probe-*` directories in `SIMSAPA_DIR`. `TempDir` cleans up on
  drop, but a killed process leaves them forever and nothing reclaims them.
  Age-gate it (e.g. older than an hour) so a concurrent import is never swept.
  Log what it removes.

  `sweep_orphaned_extract_dirs()`, called from `init_app_data()` on a background
  thread (a directory walk on cold mobile storage that nothing at startup waits
  on). The two prefixes are now the constants `EXTRACT_TEMP_PREFIX` /
  `PROBE_TEMP_PREFIX`, and the probe prefix is deliberately a *prefix of* the
  extract prefix, so one `starts_with` test covers both. Age-gated at an hour —
  and **an unreadable timestamp is treated as "too young"**, i.e. left alone:
  deleting a running import's extraction directory underneath it is much worse
  than leaving a stale folder for one more launch.
- [x] 5.7 Req. 20 (`std::env::temp_dir()` vs `QStandardPaths::TempLocation`) is a
  **non-issue** — measured identical on an Android 16 phone and on ARC
  (`staging_roots_differ: no`). Do not "fix" it. Record that in the code comment
  at `delete_temp_import_folder` so it is not re-investigated.

  Recorded, together with the second thing about that function that keeps being
  re-derived: it wipes the *root*, not a per-feature subfolder, which is why the
  dictionary path does not use it (see 5.5).

**Verification.** `cd backend && cargo test` — all suites green (558 lib +
every integration binary). `make qml-test` — 172 passed, 0 failed, and no new
`qmllint` warning naming either touched QML file. `make build -B` — clean.

**Android cross-check: ~~not verified~~ verified 2026-08-26.** Nothing in
task 5.0 touches `#[cfg(target_os = "android")]` code — `cleanup_staged_file`
and `sweep_orphaned_extract_dirs` are platform-independent — so the desktop
compile already covered every line added here. The cross-check was run anyway
and passes; see the note under task 6.0.

### 6.0 [x] Make the dictionary import actually work — the automatic picker fallback (E-4, E-7, E-14…E-17)

**Specs to keep in mind.** §0.3: Qt's `FileDialog` returns nothing on this
device while our raw intent works perfectly. We do not know which of the four
deltas is responsible, and the user should not have to wait for another round
trip to find out. So the import **tries Qt's dialog and falls back**, and the
fallback firing *is* the measurement.

- [x] 6.1 Drop `nameFilters` from `DictionaryImportDialog.qml`'s `file_dialog`
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

  Done as written: `nameFilters: Qt.platform.os === "android" ? [] :
  ["StarDict archives (*.zip)"]`, one line, commented with what it tests and
  what would revert it. The `title` is kept and now carries a comment saying
  why (with no filter the picker lists every file).
- [x] 6.2 Add the empty-URL guard (E-7): if `selectedFile` is empty or invalid,
  **never** call `scan_source` with `""`. This is what produced `Path not found: `
  with nothing after the colon. It must be distinct from the existing
  *"Could not access the selected file."*

  The guard is in the new `handle_picked_url()`, which `onAccepted` now calls
  instead of `begin_staging` directly, so **every** pick goes through one place.
  The wording is *"The file chooser did not return a file."* — a different
  failure from *"Could not access the selected file."*, which means a file was
  named and could not be read. On Android the guard does not end there; it hands
  over to 6.3.
- [x] 6.3 On Android, when the guard fires, **automatically retry with the raw
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

  All four requirements met. What the implementation settled beyond them:

  - **The discriminator is on the slot, not beside it.** `RawPickConsumer`
    (`FileSelectionTest` / `DictionaryImport`) is stored *with* the thread
    handle in the renamed `RAW_PICK_TARGET`, so the consumer cannot be read
    without the handle it belongs to, and a stale consumer from a previous run
    is not representable. `on_raw_pick_finished()` branches on it once and
    hands off. No second mechanism, no polling, and the cancelled path still
    completes — an unanswered pick would leave the dialog on the copying frame
    forever.
  - **The recovered URI is staged as a string, never re-wrapped in a `QUrl`.**
    New `DictionaryManager::stage_picked_uri(&QString)`, sharing the whole
    worker half with `stage_picked_file` through `spawn_staging(request)`. This
    is the point of the fallback: `QUrl(uri.toString())` is the conversion under
    suspicion, so routing the picker's own string back through it would put it
    straight back on the path it was recovered from. Scheme detection splits on
    `://`, never a bare `:` (§9.6).
  - **The notice is a `Dialog`, and the fallback starts from its `onAccepted`.**
    Not a toast and not automatic: a second picker appearing unannounced reads
    as a bug. Cancel returns to the source frame with the E-7 message. Clamped
    to `Overlay.overlay` with a `DialogHeader`, per CLAUDE.md.
  - **§11 Q0a was amended in the PRD, as required.** The first term of the
    2026-08-07 reversal — "confined to the diagnostic" — is now explicitly
    dropped, with the reason, the bounded cost, the three surviving terms, the
    new discriminator term, and the condition under which the dependency can be
    removed again (the fallback never firing once 6.1's filter is gone).
- [x] 6.4 Switch the dictionary import's provider read from
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

  **Verified, not re-implemented.** Re-read at 6.4's turn: `stage_picked_file`
  → `import_staging::stage_picked_url` → `android_saf::copy_document_to_path`
  (`ContentResolver.openInputStream`), with the URI coming from
  `QUrl::to_encoded()`. The fallback path added by 6.3 reaches the same reader
  through `stage_picked_uri`, so both pickers read the same way and the picker
  moving did not leave the reader behind. `QFile(content_uri)` survives in
  `cpp/utils.cpp` for the three unmigrated call sites only.
- [x] 6.5 Log a `DICTIONARY-IMPORT-PICK:` block on the real import path
  (E-14…E-17), through the **same** `PickerUrlFacts` pipeline as the diagnostic —
  never a second report shape. Log on the **failure** path too (E-15). Do **not**
  perform the diagnostic's 4 MB provider read (E-16): staging is about to read
  the file for real. Observation only; desktop behaviour byte-identical (E-17).

  One report builder, two callers. `PickReport` (`Diagnostic` /
  `DictionaryImport`) is a field on `FileSelectionTestInput` and decides two
  things and nothing else: the log prefix, and whether the block may **read**
  the document. The free `line()` helper became a `Block { out, prefix }` so a
  line written without its prefix is not expressible — the alternative, passing
  a prefix argument to every call, is one forgotten argument away from a block
  that greps as the wrong feature.

  Where the diagnostic reads, the import block prints
  `provider_read: (not read here: staging is about to copy the whole file, …)`
  — a stated measurement rather than a missing field (E-16). Both the QUrl and
  the raw-URI probes are gated, since the fallback path has both.

  **A new `filter_config` line** carries E-1/E-3's labelling discipline: every
  block now states its picker *and* its filter, and the three configurations
  are three separate literals — the diagnostic's, the raw intent's
  (`RAW_INTENT_FILTER_CONFIG`) and the import dialog's `filter_config`
  property. Nothing derives its configuration from anything else, so 6.1's
  change to the import dialog cannot silently move what the diagnostic
  measures.

  Desktop is byte-identical in effect: the logging is a `thread::spawn` that
  returns nothing the caller acts on (the block collects a directory census and
  a `statvfs`, so it must not run on the GUI thread).
- [x] 6.6 Fix `outcome_line()` (`backend/src/picker_url.rs:843`) so a success
  does not read as a failure (§0.3.7). It is currently a function of the *input*
  and cannot see whether the read worked; pass the result in. On Android every
  successful pick is `PickerBranch::Provider`, and the only cheerful arm
  (`LocalFile`) is unreachable there — so **no Android user can see a line that
  sounds like it went well.** Suggested: *"The file chooser worked. Simsapa
  opened «all-dictionaries-gd.zip» (172 MB) and read it successfully."* Leave the
  scheme in the log.

  The result is now passed in: `build_pick_report()` returns
  `PickReportOutput { block, probe }` and `outcome_line(input, probe)` takes the
  probe, so the sentence is a function of what the read *did*. The `Provider`
  arm has four outcomes — read succeeded (with the display name and size, e.g.
  *"The file chooser worked. Simsapa opened «all-dictionaries-gd.zip»
  (172.4 MB) and read it successfully."*), opened but read nothing, could not
  open, and no read attempted (the old mechanism sentence, now reachable only
  when there is genuinely no result to report). The scheme stays in the log and
  in the two failure sentences, where naming it is informative.

  `run_file_selection_test()` keeps its `-> String` shape so the existing
  20-odd tests are unaffected; it is now a one-line wrapper over
  `build_pick_report()`.
- [x] 6.7 Verify no new Android permission and that
  `android/AndroidManifest.xml` is **byte-identical** (Req. 26). Qt's dialog and
  our intent both need nothing.

  `git diff --quiet android/AndroidManifest.xml` → clean, and `git status
  --porcelain android/` is empty. No Qt module was linked, so nothing new is
  injectable even in principle — and the markers that used to inject
  permissions are gone from the manifest anyway
  (`docs/android-multi-abi-and-chromeos.md`). `ACTION_OPEN_DOCUMENT` requires
  no permission: the picker grants per-URI access to the chosen document.

**Verification.** `cd backend && cargo test` — all suites green (561 lib +
every integration binary, 0 failed), including three new `picker_url` tests:
`the_import_block_uses_its_own_prefix_and_the_same_shape`,
`the_import_block_never_reads_the_document` and
`a_successful_provider_read_reads_as_a_success`. `make qml-test` — 172 passed,
0 failed, and `make qml-lint` produces no warning naming
`DictionaryImportDialog.qml` or either stub file. `make build -B` — clean.

**Android cross-check: ~~not verified~~ verified 2026-08-26 — and it should
never have been skipped.** The sibling task list's §Notes recipe works as
written with NDK `27.3.13750724`; `cargo check --lib --target
aarch64-linux-android` finishes clean in ~75 s. The earlier "fails in `ring`'s
build script" note described a run **without** the three `CC_`/`CXX_`/`AR_`
variables the recipe exists to set.

This matters beyond bookkeeping. Task 4.1 added ~200 lines to
`backend/src/android_saf.rs` (`document_metadata`, `copy_document_to_path`,
`parse_uri`) that are `#[cfg(target_os = "android")]` and therefore invisible to
**every** desktop compile and every test in this repo. "Nothing added here is
inside `cfg(android)`" was true of tasks 5.0, 6.0 and 6.8 individually, but the
staging feature as a whole rests on code no compiler had seen. **Re-run the
recipe after any edit to `android_saf.rs`** — it is 75 s, and it is the only
check that file gets.

**Still not verified: the device behaviour.** Whether the fallback fires is the
measurement, and only the user's device can take it (task 8.7).

### 6.8 [x] Review pass — bundle archives, and five defects found by tracing the flows

A review of tasks 1.0–6.0 against the PRDs (2026-08-26) found one defect that
would have mislabelled the user's data and five smaller ones; a second pass over
those fixes found seven more. All are fixed here, before task 7.0.
**`cargo test`: 60 suites, 0 failed. `make qml-test`: 172 passed, 0 failed.
`make qml-lint`: no warning naming a touched file. `make build`: clean.**

**Android cross-check: ~~not verified~~ verified 2026-08-26.** Nothing in 6.8 is
inside `#[cfg(target_os = "android")]` — the zip work, the staging sweep and the
status plumbing are all platform-independent — and the cross-check passes too;
see the note under task 6.0.

- [x] 6.8.1 **A bundle `.zip` now imports as a folder of dictionaries would.**
  This is the big one. `-gd` releases are routinely one zip with a folder per
  dictionary — the reporting user's `all-dictionaries-gd.zip` is named as one —
  and both the old code and task 5.1's probe reported exactly **one** candidate:
  whichever `.ifo` they met first. Every other dictionary in the archive was
  unreachable.

  Worse, the two disagreed on *which* one. The probe read the zip's **central
  directory**; `import_user_zip` extracted everything and took the first `.ifo`
  the **filesystem** enumerated. The two orders are unrelated, so the checklist
  could offer dictionary A's title and word count and the import could insert
  dictionary B — under the label the user typed for A. Before task 5.1 both went
  through `locate_stardict_dir` on the extracted tree, so they agreed by
  construction; making the probe cheap is what broke the agreement.

  The fix is the shape the task's premise already had: `probe_zip_candidates()`
  (plural) returns one `ProbeOutcome` per member, `CandidateMeta` carries the
  `member` folder, and `import_user_zip_member()` extracts **only that member's
  entries**. Three consequences worth stating:

  - **The member is decided once, by the probe, and handed back at import
    time.** Re-deriving it is the defect. The value travels
    `scan_source` → `scanFinished` JSON → `DictionaryImportRow.source_member` →
    the batch item → `import_zip(path, member, label, lang)`.
  - **Importing all N members costs one archive's worth of extraction**, not N,
    because each import extracts its own folder only. That is what let the
    existing sequential batch driver stay exactly as it is.
  - **A single-dictionary archive is untouched**: `member` is `None`, the whole
    archive is extracted as before, and the label still comes from the zip's own
    filename. Only a bundle gets per-member labels, taken from the member folder
    (the zip filename is shared, so it would make every row a duplicate of the
    others). The row's subtitle names the member, since two rows of a bundle
    share a `source_path` and nothing else would tell them apart.

  `member: Some("")` — a dictionary loose at a bundle's root — maps to "whole
  archive" at the bridge. It still imports the right dictionary
  (`locate_stardict_dir` looks at the root first); it is only less economical,
  and it is a shape no real bundle has.

  Tested by `a_bundle_zip_scans_and_imports_one_dictionary_per_member` (two
  synthetic dictionaries of *different sizes* in one zip, importing the second
  and asserting its entry count — an import that took the wrong member fails on
  the number, not just the title), `a_single_dictionary_zip_reports_no_member`,
  and six unit tests over `stardict_members_in` / `entry_belongs_to_member` /
  `member_label` / the member-filtered extraction.

  **Still true, and not in scope here: a bundle is imported one dictionary at a
  time and every row is a separate `dictionaries` row.** That is what the
  checklist has always meant. The 8.7 covering message should say so, or the
  user will read "1 of 12 imported" as a partial failure.
- [x] 6.8.2 **`.IFO` probed as valid and then failed to import.**
  `is_shallow_ifo_entry` lowercased the extension; `find_ifo_stem_in` compared
  `extension() == Some("ifo")` exactly. Now both are case-insensitive.
- [x] 6.8.3 **Two staged-copy leaks.** `onScanFailed` never discarded the staged
  file, and staging a *second* file in one dialog session overwrote
  `staged_path` without discarding the first — a whole archive, up to hundreds
  of MB, with nothing left holding its path. `enter_copying_frame()` now
  discards first, and so does `onScanFailed`.

  Task 5.6 swept orphaned **extraction** directories but nothing swept the
  **staged copies**, which are the larger files: a process killed mid-copy (the
  Android low-memory killer during a 180 MB read is the case) left the whole
  archive behind forever. `import_staging::sweep_orphaned_staged_files()` is the
  twin sweep, same one-hour age gate, same "an unreadable timestamp counts as
  too young" rule, called from `init_app_data()` beside the other.
- [x] 6.8.4 **The search-UI empty state now reports the *area* that was
  searched.** `FulltextState::CouldNotOpen` requires zero indexes open across
  *all three* areas, so a user whose sutta indexes open and whose dictionary
  indexes all fail was back to a silent "No results found." on every dictionary
  search — FR-23…FR-25's defect, narrowed to one area. `FulltextAreaStatus`
  gained a `failed` count (recorded where the failure happens, **not** derived
  later by matching `/suttas/` against a path, which would break on a storage
  location containing that word), and `fulltext_status` gained `area_state()` /
  `area_message()`. `FulltextResults` picks the block for `search_area`, whose
  property was declared and unread until now. Every sentence still comes from
  `fulltext_status.rs`, so `no_jargon_in_user_facing_strings` still covers them
  all — it now checks the per-area ones too.
- [x] 6.8.5 **Database Validation could invent a fulltext failure.** The comment
  claimed the row was emitted "after the searcher has had its chance to open",
  but `load_searcher()` is a separate spawned thread and nothing sequences the
  two. On a launch where the update check fails fast (no network), validation
  wins the race and reports *"The search index has not been opened yet."* as a
  **failure**, logged at ERROR — in the one report the user is asked to send,
  and in the exact build whose returned log is the measurement. Now
  `init_fulltext_searcher()` (idempotent) runs first.
- [x] 6.8.6 **`stage_picked_uri`'s `file://` branch** used
  `trim_start_matches("file://")`, which yields `/C:/x` on Windows and leaves
  percent-escapes in. It goes through `qurl_to_local_path` now, the same
  conversion the Qt-side entry point uses. Unreachable today (the raw pick is
  Android-only), but it was wrong where it claimed to be complete.
- [x] 6.8.7 **`INDEX_VERSION` is now load-bearing, and says so.** Task 1.2's
  `Index::open` dropped Tantivy's schema-equality check —
  `Index::open_or_create` returned `SchemaError` on a mismatch, `Index::open`
  takes whatever is on disk. A stale index therefore opens silently and fails
  per query instead of being recorded as an open failure. Nothing is broken
  today because `is_index_current()` offers a rebuild, but that is now the
  **only** guard: recorded on the constant itself and in
  `docs/fulltext-index-storage-and-file-locking.md`.

**Second pass over 6.8 itself** — reviewing the fixes found six more, four of
them in the new code:

- [x] 6.8.8 **The probe and the import could still disagree inside one folder.**
  6.8.1 fixed the *between*-folder case and left the *within*-folder one: a
  folder holding two `.ifo` files was resolved by "the first one", which meant
  central-directory order in `stardict_members_in` and **`read_dir` order** in
  `find_ifo_stem_in` — neither specified, and not each other. Both now take the
  lexicographically smallest name, which also makes a two-`.ifo` folder import
  the same dictionary every time rather than whatever the filesystem listed
  first.
- [x] 6.8.9 **A member at a bundle's root would have lost its `res/`
  resources.** `entry_belongs_to_member(_, "")` filtered to root-*level* entries,
  and a root dictionary's resources live one level down in `res/`. An empty
  member now means "the whole archive" — the same mapping the bridge already
  applied — and the probe reports `None` rather than `Some("")` for such a
  member, so the case is not representable end-to-end. `locate_stardict_dir`
  looks at the root before any subfolder, so the right dictionary is still the
  one imported.
- [x] 6.8.10 **One bad dictionary in a bundle read as a bad archive.** The
  rejection is rendered under the *archive's* name, so a failing member produced
  `"all-dictionaries-gd.zip" its description file could not be read.` about an
  archive whose other eleven dictionaries were fine. `extract_one_entry` now
  returns a typed `EntryReadError` that the caller — the only place that knows
  which member it was — turns into a sentence naming it.
- [x] 6.8.11 **An area is not all-or-nothing either.** 6.8.4 fixed
  "sutta opens, dict fails" and left "`suttas/en` opens, `suttas/pli` fails":
  the area's state is `Ready`, so a Pāli search was back to a silent "No results
  found." while English worked. `area_message()` now covers three cases, and
  **an empty message is the whole instruction to stay silent** — QML no longer
  branches on the state at all, so the next case like this is a backend change
  only.
- [x] 6.8.12 **Database Validation could print "All checks passed" over a broken
  index.** `is_valid` meant "search works at all", which is `state`'s job; a
  partly-open index was therefore reported as clean. It now means *everything
  that should have opened, opened* — the two fields answer different questions
  and both are documented as doing so. `/health` is unaffected
  (`fulltext_searcher_ready` is computed from the counts, not from this) and
  gained the per-area `failed` / `state` / `message` fields, built through the
  same helpers the app's own UI reads so the two cannot drift.
- [x] 6.8.13 **The staging feature name was four literals.** Staging,
  `cleanup_staged_file` and `sweep_orphaned_staged_files` all key on it, and a
  mismatch fails **silently** in the worst way: ownership is decided by
  location, so a cleanup pointed at the wrong folder refuses every delete
  without an error and the sweep watches a folder nothing writes to. Now
  `import_staging::DICTIONARY_FEATURE`.
- [x] 6.8.14 **`onScanFailed`'s abandoned branch leaked the staged copy**, while
  `onScanFinished`'s abandoned branch discarded it. Same branch, same rule.

**One test was fixed, and it was the test that was wrong.**
`test_dict_word_headword_match_with_language_filter` asserted that page 0 of a
`pli`-filtered Headword Match for "dhamma" contains a `/dpd` row. Headword Match
orders by match tier then by `dict_label`, and the dev DB has since gained
`cone-gd` (37k rows, 157 of them matching, sorting before `dpd`) — so the first
~15 pages are `cone-gd` and the assertion failed on ranking, which is not what it
tests. It now walks pages until it finds one or runs out. **This failure predates
this branch**: nothing in it touches `query_task.rs`, the schema or the FTS
scripts.

### 6.9 [x] Second review pass — four defects and two stale claims

A review of tasks 1.0–6.8 against the PRDs (2026-08-26, after 6.8) traced the
search-UI, validation, staging and scan flows end to end. Four defects, one
hardening, and two documented statements that had become false.
**`cargo test`: 60 suites, 0 failed. `make qml-test`: 172 passed, 0 failed.
`make qml-lint`: no warning naming a touched file. `cargo check --lib --target
aarch64-linux-android`: clean.**

- [x] 6.9.1 **The search-UI empty state could blame the index for a search that
  never used it.** `check_fulltext_index_problem()` gated on the searched
  *area* and on the backend's `state`, but never on the search **mode**.
  Contains Match, Title Match, Headword Match and DPD Lookup all go through
  FTS5/SQLite and work perfectly on a volume where every Tantivy index failed
  to open — which is the reporting user's exact configuration. A Contains Match
  that genuinely matched nothing would have read *"The search index could not be
  opened. This storage location does not support the file locking the search
  index needs. Open Database Validation from the menu for details."*

  That is a fabricated diagnosis of a search that touched no index, and it is
  the same class of dishonesty the whole 2.0 task exists to remove — pointed the
  other way. Fulltext PRD **FR-23 scopes the message to FulltextMatch and
  Combined**, and the gate was simply missing.

  `FulltextResults` gained a `search_mode` property and
  `uses_fulltext_index()`, checked **first** in
  `check_fulltext_index_problem()`. It is an **allowlist**
  (`"Fulltext Match"`, `"Combined"`), not a denylist of the FTS5 modes: a mode
  added later then stays silent by default, which costs nothing, where a new
  FTS5 mode silently inheriting "the index could not be opened" is this defect
  returning. `"Combined"` is in it because the Dictionary combined page's third
  stream *is* a Fulltext Match (`docs/search-snippet-highlight-pipeline.md` §9).
  An empty mode — QML preview, or a page with no search behind it — is treated
  as not using the index.

  `SuttaSearchWindow` supplies it from `root.last_params.mode`, the same object
  `new_results_page()` replays, so a page navigation reports the mode its
  results actually came from rather than whatever the dropdown now shows.
- [x] 6.9.2 **`init_fulltext_searcher()` was check-then-act, and 6.8.5 added a
  second concurrent caller to it.** It took a read lock, saw `None`, dropped it,
  then opened. `SuttaBridge::load_searcher()` spawns one thread at startup and
  `dictionary_first_query()`'s validation — as of 6.8.5 — spawns another that
  calls the same function; both could pass the check.

  Two openers is not merely wasteful. `FulltextSearcher::begin_open_session()`
  **clears** `SEARCHER_OPEN_FAILURES`, so the second opener's clear can wipe the
  first's recorded failures, leaving zero indexes open *and* zero failures
  recorded — which `build_status` classifies as `FilesNotFound`: *"Fulltext
  index files not found. Use Rebuild Search Index to create them."* over a
  volume whose indexes are all present and all failed to open. 6.8.5 removed one
  fabricated failure from the report the user is asked to send and opened a
  narrow window onto another.

  New `SEARCHER_OPEN_LOCK` in `backend/src/lib.rs`. `init_fulltext_searcher()`
  keeps its lock-free fast path, then **re-checks under the lock** — without the
  second check the lock buys nothing — and `reinit_fulltext_searcher()` takes it
  too, since two reinits from different features (a reconcile and a rebuild)
  clobber each other's failure list the same way. The open itself moved into a
  private `open_fulltext_searcher()` whose contract is "callers hold the lock".
- [x] 6.9.3 **A scan's rejections were invisible whenever it also found
  something.** `onScanFinished` read `report.rejections` only in the
  `candidates.length === 0` branch. A folder holding three StarDict zips and one
  MDict, or a bundle archive whose twelfth member is corrupt, produces both at
  once — that is what `ScanReport` is a struct and not an enum *for* (5.2) — and
  the refused files vanished without a word.

  This is the reporting user's own complaint (§0.3.6): two archives side by
  side, one importable and one not, and nothing in the app saying which. Task
  5.2 built the data to answer it and the dialog only rendered it on the one
  path where there was nothing else to show.

  New `scan_rejections` property, rendered on the checklist frame above the
  list: *"N items were skipped:"* followed by the backend's own sentence per
  item, verbatim — that is the only place that knows whether it was an MDict
  file, an unreadable archive or a failure on our side. Hidden entirely when
  there is nothing to say; an unconditional "0 skipped" reads as a fault where
  there is none. Cleared in `start()` **and** in `begin_scan()`, so a second
  scan that finds nothing cannot leave the first scan's refusals on screen.

  **The five-line cap is load-bearing, not tidiness.** The block sits above the
  checklist's `Layout.fillHeight` ScrollView, so every line it prints is a line
  taken from the dictionaries the user came to tick — a folder of twenty
  unreadable files would bury the three good ones. The remainder collapses to
  "and N more (see the log file for the full list)", and every rejection is
  already logged individually by `scan_source`.
- [x] 6.9.4 **Closing the import window stranded the staged copy.** Every button
  path calls `discard_staged_file()`; the window-manager close button and
  Android's back gesture — the two exits no Cancel handler covers — did not. A
  whole archive, up to hundreds of MB, sat in the staging folder until the next
  `start()` or the hourly startup sweep of 6.8.3 reclaimed it. One
  `onClosing: root.discard_staged_file()`. Safe on every path: it no-ops with no
  staged copy, the backend refuses a path outside the dictionary staging folder,
  and Import has already handed ownership to `DictionariesWindow` before it
  hides the window.
- [x] 6.9.5 **Hardening, not a live defect: `to_json` escaped one string out of
  seven.** The whole-app `message` went through an escape; the six per-area
  sentences did not. All are written in `fulltext_status.rs` and none contains a
  quote, so the output was correct — but that is a property of today's wording,
  not of the code, and the failure mode is silent: `JSON.parse` throws inside
  QML's `try`, the `catch` clears the message, and the broken index goes back to
  being invisible. One `json_escape()` helper, applied to all seven.
  `every_json_string_is_escaped` pins it and round-trips the real document
  through serde, which also catches a malformed field in the hand-written
  `concat!` template.

  Also: `DatabaseValidationDialog.run_validation_checks()` now clears
  `fulltext_failed` / `fulltext_message` alongside the three database flags. The
  fulltext result rides the dictionaries signal and is always re-emitted, so
  nothing was broken — but that made this row's correctness a property of
  another function's control flow.

**Two documented statements had gone stale, and both were corrected.**

- `docs/storage-diagnostics.md` §Section F still said
  `is_fulltext_searcher_ready()` "returns `true` whenever the global is `Some`
  regardless of index count… fixing it is phase-2 work". Task **2.4 is** that
  fix. The paragraph now records the change and keeps its actual rule, which is
  unaffected: "was this ever measured this session" is a third question, still
  answered only by `with_fulltext_searcher()` returning `None`.
- **The Android cross-check was never blocked.** Tasks 5.0, 6.0 and 6.8 each
  carried "fails in `ring`'s build script without the NDK environment". The
  sibling task list's §Notes recipe works as written with NDK
  `27.3.13750724` — `cargo check --lib --target aarch64-linux-android` finishes
  clean in ~75 s; the earlier note described a run **without** the three
  `CC_`/`CXX_`/`AR_` variables the recipe exists to set. Each caveat is
  corrected in place. This matters: task 4.1 added ~200 lines to
  `android_saf.rs` (`document_metadata`, `copy_document_to_path`, `parse_uri`)
  that **no desktop compile and no test in this repo ever sees**. Re-run it
  after any edit to that file.

### 7.0 [x] Say "StarDict/GoldenDict" everywhere the UI says "StarDict"

**Scope reduced to this one line 2026-08-26, on the user's instruction.** The
format *detection* work is not needed, and neither is a new "what to do next"
sentence.

The problem it answers is still §0.3.6: the user had a valid StarDict file and
an MDict file and could not tell them apart. Many users know the format only as
"GoldenDict" — the reporting user's own file is `all-dictionaries-gd.zip` — so
every place the UI says "StarDict" alone fails to connect to what they are
holding. Naming both is the whole fix.

**7.1 and 7.3 already landed inside task 5.2** — verified 2026-08-26.
`ArchiveFormat` / `detect_archive_format()` name the format *found* (MDict
`.mdx`/`.mdd`, DSL, XDXF, "a zip of something else"), the `scan_source`
URL-scheme rejection is in (Req. 15), and both are unit-tested. The three
failure classes are already distinguished by `ScanRejection.reason`
(`unsupported_format` / `unreadable` / `io_failure`) and already render as three
different sentences. **Do not add more detection, and do not add fixture
archives.**

- [x] 7.1 ~~Recognise archive contents by entry name~~ — **done in 5.2**
  (`detect_archive_format`, unit-tested). No further work.
- [x] 7.2 Replace "StarDict" with **"StarDict/GoldenDict"** in every
  user-facing string. All nine, found by one grep over `assets/qml/`,
  `bridges/src/` and `backend/src/`:

  | Where | String |
  |---|---|
  | `DictionariesWindow.qml:504` | the toolbar button, `Import StarDict/GoldenDict...` |
  | `DictionaryImportDialog.qml:28` | the window title |
  | `:188` | `filter_config`, the logged picker configuration |
  | `:417` | `No StarDict/GoldenDict dictionaries were found in the chosen source.` |
  | `:458` | the file dialog's title |
  | `:468` | `nameFilters` — the desktop file dialog's filter label |
  | `:535` | the source frame's heading |
  | `:779` | the scanning frame's label |
  | `dictionary_manager_core.rs:1152` | `"…" does not contain a StarDict/GoldenDict dictionary.` |

  `filter_config` is included because it is a **verbatim statement of the
  filter literal** for the `DICTIONARY-IMPORT-PICK:` log block (6.5); leaving it
  behind would make the logged configuration disagree with the one in force.
  Comments and doc comments were left alone — they name the format, not the UI.
- [x] 7.3 ~~`scan_source` rejects a URL-scheme string~~ — **done in 5.2**
  (`://`, never a bare `:`).
- [x] 7.4 ~~Unit-test the classifier with fixture archives~~ — **descoped with
  the detection work.** The classifier's existing unit tests over entry-name
  lists stay; no fixture archives are built.

**Verification.** `cd backend && cargo test` — 60 suites, 0 failed.
`make qml-test` — 172 passed, 0 failed. `make qml-lint` — no warning naming
either touched QML file. `make build -B` — clean. No Android cross-check needed:
nothing here is inside `#[cfg(target_os = "android")]`.

### 7.5 [x] A bundle is a zip of **zips** too — `all-dictionaries-gd.zip` was rejected outright

**Confirmed on device 2026-08-26 by the user: `all-dictionaries-gd.zip` now opens
and imports on both desktop and Android.** That closes the import half of §0.3 —
the picker, the staging and the bundle probe are all exercised by that one run.

Found by the user on 2026-08-26, testing the 6.8.1 work on the real archive:
`all-dictionaries-gd.zip` failed on **both** desktop and Android with *"does not
contain a StarDict/GoldenDict dictionary"*, while `cone-gd.zip` from the same
release imported fine.

6.8.1 assumed one bundle shape — a **folder** per dictionary. The archive the
whole report is about has the other: **a `.zip` per dictionary**, 14 of them, all
`Stored`, no `.ifo` anywhere in the outer central directory. So
`stardict_members_in()` found nothing and the archive was classified
`UnsupportedFormat(Unknown)` — the one outcome that reads as "this is not a
dictionary" about a file that is fourteen of them.

- [x] 7.5.1 **Both shapes are read.** `nested_zip_entries_in()` lists `.zip`
  entries by the same two-level rule `is_shallow_ifo_entry` uses (root or one
  folder deep — whatever the scan offers, the import has to reach), and each is
  probed by the same `.ifo` read. `is_bundle` counts folder members **plus**
  nested archives. Directory entries (a trailing `/`) are excluded: a folder
  named `foo.zip` belongs to `stardict_members_in`, and probing it as an archive
  would file a bogus rejection beside the candidate it legitimately produced.
- [x] 7.5.2 **A nested archive is opened in place, not copied out.** `FileSlice`
  is a `Read + Seek` view of one byte range of the outer file; a `Stored` nested
  entry is already a contiguous run of bytes, so `ZipArchive::new` can read it
  where it lies. The range length is the entry's `compressed_size` — the bytes
  actually on disk — not `size`. `open_nested_archive()` copies the entry into
  the probe's temp folder only when it is **deflated**, because a deflate stream
  cannot be seeked. Measured on the real archive: **14 candidates in 7 ms**,
  nothing written but 14 `.ifo` files. Copying the nested archives out instead
  would have written 180 MB per scan, on a phone.
- [x] 7.5.2b **A copy, where one is needed, dies with the archive that needed
  it.** `NestedArchive::discard()` drops the handle *then* deletes the file
  (Windows refuses to delete an open one), and both call sites discard before
  moving on — the probe per nested archive, the import before it reads the
  extracted tree. Holding them to the end of a scan would mean the entire bundle
  in temp files simultaneously. Two accepted limits of the deflated fallback, on
  a shape no measured bundle has: the copy is not cancellable and reports no
  progress, and the import's copy sits in the extraction temp dir (harmless —
  `locate_stardict_dir` looks for an `.ifo`, never a `.zip`).
- [x] 7.5.3 **The member string carries the nested entry, jar-style.**
  `"abt.zip!/"`, or `"abt.zip!/pts"` when a nested archive itself holds several
  dictionaries. The separator is **always** written, so `split_nested_member()`
  never guesses from the `.zip` extension — a *folder* named `whatever.zip` is a
  legal bundle member. The split is on the **last** `!/`, which is exact because
  a folder member never contains a `/` (`stardict_members_in` cannot produce
  one), so an outer entry like `weird!/abt.zip` still round-trips. Nothing on the
  QML side changed: the member is opaque there, and always has been.
- [x] 7.5.4 **A nested member's `member` is always `Some`**, bundle or not.
  Unlike a folder member it cannot be reached by extracting the outer archive —
  that yields `.zip` files and no `.ifo`. The label comes from the nested
  archive's own filename (`abt.zip` → `abt`), which is the only thing telling two
  rows of one bundle apart.
- [x] 7.5.5 **A bad nested member is named as a member, never as the archive.**
  *"…contains "mdict.zip", which is an MDict dictionary (.mdx)."* One bad member
  among twelve good ones must not read as "this archive is not a dictionary" —
  the same rule 6.8.1 established for folder members.
- [x] 7.5.6 **`DictionaryImportRow.member_display`** renders `abt.zip!/` back as
  `abt.zip` (and `abt.zip!/pts` as `abt.zip / pts`) in the row subtitle. The
  separator is for the import, not for the eye.
- [x] 7.5.7 **The source-selection wording says a `.zip` may be a bundle.**
  `"A single dictionary .zip archive"` told users a bundle was the *wrong* choice
  on that screen — when for a bundle it is the only choice there is. Now
  `"A .zip archive — one dictionary, or a bundle of several"`, with a note under
  the options defining a bundle in the user's terms (one `.zip` containing
  several dictionaries, each in its own `.zip` file **or** folder inside it) and
  saying what happens next: every dictionary is listed, each imported separately
  with its own name. That sentence is the one that stops a dozen rows reading as
  a fault, and it now appears where the choice is made rather than only in the
  covering e-mail. The mobile note's *"Multiple dictionaries have to be imported
  one at a time"* was also wrong as of 6.8.1 — a bundle is one pick and every
  dictionary in it can be ticked at once; what mobile cannot do is choose a
  **folder**, which is what it now says.
- [x] 7.5.8 **The re-indexing window centres its content.**
  `DictionaryIndexProgressWindow` — the startup screen showing
  `Indexing: 3/14 cone, 21099/37391 words`, which is where a bundle import is
  actually paid for — had one bottom spacer, so on a full-screen mobile window
  the label and bar sat against the top edge with the rest of the screen empty.
  A second spacer above centres them.

**Verification.** Three new integration tests (stored nesting scan + import of
the *second* member asserting its entry count, deflated nesting through the
copy-out path, a non-dictionary nested archive rejected by name) and four unit
tests (`nested_zip_entries_in` including the `foo.zip/` folder case, the encoding
round-trip including the `weird!/` case, `FileSlice` reading the nested archive
rather than the outer one, and the deflated copy being deleted on `discard()`).
`cargo test --test test_dictionary_import_dir` — 10 passed. `cargo test --lib
dictionary_manager` — 19 passed. Full `cargo test` — no failures. `make qml-lint`
— no warning naming the touched file. `make build -B` — clean.

**Measured against the real archive** (`/home/gambhiro/Downloads/temp/
all-dictionaries-gd.zip`, 180 MB, via a temporary `#[ignore]`d test since
removed): all 14 dictionaries listed with correct titles and word counts in 7 ms,
zero rejections; importing `nyanatiloka.zip!/` inserted its 1405 entries and
captured its one `res/` file in 2 s, leaving no `simsapa-stardict-*` directory
behind.

**Not a defect, but this archive will show it:** its `dppn.zip` suggests the
label `dppn`, which is a **shipped** dictionary. The row is flagged
`taken_shipped` by the existing per-row `check_label_status` *before* any import
runs, so it is a rename in the dialog, not a mid-batch failure. Worth saying in
the 8.7 covering message alongside "a bundle is N rows".

### 8.0 [ ] Tests, docs, and the build to send

- [x] 8.1 `cd backend && cargo test` and `make qml-test` pass. New unit tests
  from 1.6, 3.x, 5.4 and 7.4.

  `cargo test` — **60 suites, 0 failed.** `make qml-test` — **172 passed, 0
  failed.** `make qml-lint` — the pre-existing 41-warning baseline, none naming
  a file touched by this branch. `make build -B` — clean.
  `cargo check --lib --target aarch64-linux-android` — clean.

  New tests, by the task that added them: 1.6 (three `lenient_directory` unit
  tests over the fallback route, sharing `pretend_flock_is_unsupported()`), 3.x
  (`test_lenient_directory_benchmark.rs` — the ratio benchmark carrying 3.4's
  per-directory probe-count assertion, plus the Linux-only watcher-thread
  check), 5.4 (the hand-built path-traversal archive), 6.0 (three `picker_url`
  tests over the shared report pipeline), 6.8 (the bundle scan/import
  round-trip plus six unit tests over member resolution), 6.9
  (`every_json_string_is_escaped`), and `fulltext_status`'s
  `no_jargon_in_user_facing_strings`. **7.4 was descoped** with the format
  detection work; `detect_archive_format`'s existing unit tests over entry-name
  lists stay.
- [x] 8.2 Write `docs/fulltext-index-storage-and-file-locking.md` (fix-PRD §7):
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

  Written as §1–§7, in front of the framing and benchmark that task 3.7 already
  put there (now §8–§9). All four cross-links are in;
  `docs/storage-diagnostics.md` and
  `docs/simsapa-localhost-api-search-endpoints.md` already carried theirs.

  **Two things the doc says that are not in the PRD, and are the reason to read
  it rather than the PRD.** First, the general lesson behind §1: *a probe proves
  only the primitive it actually used* — the tier-2 storage probe exercises a
  real SQLite database, which is `fcntl`, so it passed on a volume where every
  `flock` failed, and there was no code path anywhere in the app that touched
  `flock` before the index tried to open. Second, §7's two **dependencies the
  fix created** — `ReloadPolicy::Manual` makes an explicit
  `reinit_fulltext_searcher()` mandatory after every in-app index mutation, and
  searcher opening had to be serialised because `begin_open_session()` clears
  the failure list. Both are standing rules for future code, not history.
- [x] 8.3 Write `docs/dictionary-import-pipeline.md`: pick → stage → probe →
  import, which stage runs on which thread, the signal surface, the
  single-extraction rule, the cleanup owners (per-feature subfolder, the startup
  sweep, `TempDir` on drop), the keep-screen-on holders, and the format-detection
  table. Record the `readAll()`-on-the-UI-thread defect as **fixed** so it is not
  reintroduced.

  Eleven sections. The stage table names the thread and the signal surface for
  each of pick / stage / probe / choose / import; §2 records the `readAll()`
  defect as fixed and names what replaced it; §7 is the three temporaries and
  their owners.

  **Two things carry more weight in the doc than in the task list, because they
  are the traps that fail silently.** `cleanup_staged_file` decides ownership
  **by location, not by the caller's word** — which is what protects a desktop
  pick (the user's own archive, never copied) from any path QML passes in — and
  the staging feature name is **one constant**, because a mismatch makes the
  cleanup refuse every delete with no error while the sweep watches a folder
  nothing writes to.

  §5's bundle-archive section is written around **why** the probe and the import
  disagreed, not just the fix: making the probe cheap is what broke an agreement
  that used to hold by construction (both went through `locate_stardict_dir` on
  the extracted tree). That is the reusable lesson — an optimisation that
  changes *which* data source answers a question can break a consistency nobody
  wrote down.
- [x] 8.4 Update `docs/file-selection-test.md` (§5.3 already records the returned
  report) with whatever task 6.0 changes, and
  `docs/simsapa-localhost-api-search-endpoints.md` for the `/health` change from
  2.4. Update `PROJECT_MAP.md` and `CLAUDE.md` per the standing rule.

  `file-selection-test.md`: §3.1's "keep it confined to the diagnostic" term is
  **struck**, with the reason and the unchanged blast radius; a new §5.4 records
  the three ways phase 1b changed the feature (two callers of one report
  pipeline, the `filter_config` line, and `outcome_line()` taking the result);
  §9 gains the cross-link. `simsapa-localhost-api-search-endpoints.md` was
  already updated by 2.4 and 6.8.12 and needed nothing.

  `PROJECT_MAP.md`: two entries were **stale in a way that would have misled** —
  `lenient_directory.rs` was still described as "reached **only** from the
  storage diagnostics", and its own bullet said `searcher.rs` and `indexer.rs`
  "still use bare `MmapDirectory`". Both now describe the shipped wiring. Added:
  `fulltext_status.rs`, `import_staging.rs`, the staging/scan signal surface,
  the SAF **reader** half with its cross-compile warning, and a pointer to the
  new import doc at the head of the dictionary-management section.

  `CLAUDE.md` is a **symlink to `AGENTS.md`** — edit the target, not the link.
- [x] 8.5 Update the three source PRDs' status headers: the fulltext fix from
  "unblocked" to implemented; the picker PRD's phase-1b section with what
  actually shipped and what the fallback logging will tell us.

  - **Fulltext fix PRD** — "Draft / not yet implemented" → **IMPLEMENTED**, with
    §4.7 named as the only unimplemented part and device confirmation named as
    still outstanding. The header also lists **six things implementation added
    beyond the requirements**, each because tracing a flow found a gap the PRD
    did not anticipate — most notably that FR-17's stated premise ("every index
    mutation is already followed by an explicit reinit") was **not quite true**,
    and that FR-23 was missing a search-**mode** gate without which a genuinely
    empty Contains Match would have blamed the index.
  - **Picker PRD** — phase 1b promoted from "Planned" to **IMPLEMENTED**, with a
    table of what shipped against each E-number and Req, and phase 2's remaining
    blocked scope narrowed to the four-call-site shared-resolver migration
    specifically. §11 Q0a was already amended by task 6.3.
  - **Diagnostics PRD** — "Phase 2 is unblocked" → implemented, plus the point
    that two of its deliverables outlived the diagnostic as shipping code, and
    that the **`mmap` probe's answer deleted a planned second PRD**, which is
    the highest return that phase produced.
- [x] 8.6 Non-goal verification, by grep: `android/AndroidManifest.xml`
  byte-identical; the four call sites of picker-PRD §2.6 **unchanged except the
  dictionary one**; no `console.` in touched QML; no `qInfo`/`qWarning` added; no
  `.exists()` added; no second `ANALYZE` added; no `.unwrap()` on a
  `qt_thread.queue()`.

  All seven clean, run as `git diff <merge-base with main> HEAD`:

  | Check | Result |
  |---|---|
  | `android/AndroidManifest.xml` | `git diff --quiet` → **clean** |
  | picker-PRD §2.6's four call sites | only `DictionaryImportDialog.qml` changed; `DocumentImportDialog`, `ChantingPracticeWindow` and `GlossTab` untouched |
  | `console.` in QML | 7 additions, **all** in `assets/qml/com/profoundlabs/simsapa/` — the qmllint stubs, the documented exception |
  | `qInfo` / `qWarning` / `qDebug` | none added |
  | `.exists()` | none added |
  | second `ANALYZE` | none added (the two hits are prose in this file) |
  | `.unwrap()` / `let _ =` on `qt_thread.queue()` | none added |

  The `console.` row is the one worth stating rather than asserting: a bare
  `grep` over `assets/qml/` reports seven violations, and the exception is a
  property of the **file's directory**, not of the line. Check per file.
- [ ] 8.7 Build the beta package for the user (`make android-beta-dist` → the
  `io.github.simsapa.app.beta` package, which installs *alongside* their Play
  copy — see `docs/android-beta-distribution-and-play-policy.md`) and write the
  covering message.

  **The covering message is written and lives beside this file:**
  [`2026-08-25-190522-covering-message-fulltext-fix-and-dictionary-import-overhaul.md`](./2026-08-25-190522-covering-message-fulltext-fix-and-dictionary-import-overhaul.md).
  It carries the user-facing text and, below it, the **grep table for reading the
  returned log** — which is the half that decides the open questions. The build
  itself is still outstanding (the version bump is the user's).

  **What to ask them to do, in this order:**
  1. Import **`all-dictionaries-gd.zip`** — the one in
     `Documenti/Dizionari`. **Explicitly say: not the `mdict` one**, which
     cannot work and would produce an ambiguous result.

     **Say what a bundle archive now does** (task 6.8.1): the checklist will
     list *every* dictionary inside that zip, one row each, and each is imported
     as its own dictionary with its own label. Ticking all of them is fine.
     Without that sentence, a list of a dozen rows where they expected one reads
     as a fault — and it is the answer to the question they were actually
     asking, since before this build the archive would have imported exactly one
     of them.
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
6. ~~**Is the `.ifo`'s declared `wordcount` an acceptable substitute for
   `dict.idx.items.len()`**~~ — **answered (task 5.1): yes, and it is what is
   shown.** Required by the StarDict spec, display-only, and the alternative
   costs a full extraction. `probe_dir_candidate` was moved onto the same read,
   so a dictionary and its extracted folder now agree.
7. ~~**Does `stardict::no_cache` require the `.dict`/`.dict.dz` to be
   present?**~~ — **answered (task 5.1): yes.**
   `stardict-0.2.3/src/lib.rs:163-164` calls `get_sub_file(prefix, "dict",
   "dz")` and errors with `NoFileFound` when neither exists. So the `.idx` route
   was never cheap, and 5.1 took the `.ifo`-only route as the task anticipated.
