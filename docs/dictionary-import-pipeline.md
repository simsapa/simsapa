# Dictionary import pipeline

How a user-supplied StarDict/GoldenDict dictionary gets from a file picker into
`dictionaries.sqlite3`, which stage runs on which thread, who owns each
temporary file, and the traps that are easy to reintroduce.

Source PRD:
`tasks/2026-07-31-180502-prd---picker-url-handling-and-chromebook-import-failure.md`
(phase 1 shipped the File Selection Test diagnostic — see
[file-selection-test.md](./file-selection-test.md); this document describes what
phase 1b built on top of it). Task list:
`tasks/2026-08-25-190522-tasks-fulltext-fix-and-dictionary-import-overhaul.md`.

## 1. The five stages

```
pick  →  stage  →  probe (scan)  →  choose  →  import
```

| Stage | Runs on | Entry point | Reports through |
|---|---|---|---|
| **pick** | UI thread (the picker is the platform's) | `DictionaryImportDialog.qml` `file_dialog` / `folder_dialog`, or the Android raw intent | `handle_picked_url()` |
| **stage** | worker thread | `DictionaryManager::stage_picked_file(&QUrl)` / `stage_picked_uri(&QString)` | `stagingProgress` / `stagingFinished` / `stagingFailed` |
| **probe** | worker thread | `DictionaryManager::scan_source(kind, path)` | `scanFinished` (a `ScanReport` JSON) / `scanFailed` |
| **choose** | UI thread | the checklist frame | `import_batch_requested(items_json)` |
| **import** | worker thread, one item at a time | `DictionaryManager::import_zip(path, member, label, lang)` / `import_dir` | `importProgress` / `importFinished` / `importFailed` / `importCancelled` |

The dialog is a four-frame `StackLayout`, and the frame indices are **named
`readonly property int`s on `root`** (`frame_source`, `frame_copying`,
`frame_scanning`, `frame_checklist`) — never literals. A frame was inserted once
and every literal in the file had to move; naming them is what makes that safe to
do again.

The **batch** (running the chosen rows one after another, with a progress frame,
an Abort button and a summary) lives in `DictionariesWindow.qml`, not in the
import dialog. It predates this work and was not rebuilt.

## 2. `readAll()` on the UI thread — fixed, do not reintroduce

**The defect:** `SuttaBridge.copy_content_uri_to_temp` was a plain synchronous
invokable called straight from `FileDialog.onAccepted`, and it reached
`copy_content_uri_to_temp_file` (`cpp/utils.cpp`), which did
`QByteArray data = source.readAll()` — **the whole archive into one buffer on
the GUI thread**, then one `dest.write(data)`. At the reporting user's 180 MB
that is seconds of frozen UI and an ANR risk on a slower stream (a Drive-backed
pick on a Chromebook streams over the network).

**What replaced it:** `backend/src/import_staging.rs` — the whole
platform-independent staging half, Qt-free and unit-tested off-device. The copy
runs on a worker thread in 1 MB chunks (`CHUNK_BYTES`), checking a cancel flag
between chunks.

`copy_content_uri_to_temp` still exists for the three unmigrated call sites
(document, chanting, Gloss) and carries a comment naming its replacement. **The
dictionary path must not go back to it.**

Two shapes worth keeping:

- **`stagingProgress` carries `f64`, not `i32`.** A byte count is not guaranteed
  to fit in an `i32` (a 3 GB archive is legal), and QML numbers are doubles
  anyway, so `i32` would have been a silent wrap at the one size where progress
  matters most.
- **Progress is throttled to 100 ms, not emitted per chunk.** A 1 MB chunk of a
  local copy completes in well under a millisecond; a queued cross-thread signal
  per chunk would cost more than the copy. Nothing is lost —
  `stagingFinished` carries the terminal state.

## 3. Reading the picked file

Two readers, chosen by scheme, both reached through
`import_staging::stage_picked_url`:

| Scheme | Reader |
|---|---|
| `file://` (and bare paths) | `qurl_to_local_path` + `std::fs`. A desktop pick is the **user's own file** and is never copied — it is used in place. |
| `content://` (Android/ChromeOS) | `android_saf::copy_document_to_path`, i.e. `ContentResolver.openInputStream`, chunked |

**Never `QFile(content_uri)`** — that works only by accident and only for
`content://`; the SAF reader is the supported path. And the **`to_encoded()`
rule** applies in both directions: pass the fully-encoded URI, because `.path()`
drops scheme and authority and `toString()` pretty-decodes `%3A`/`%2F` and breaks
`Uri.parse`. See [android-file-saving-saf.md](./android-file-saving-saf.md).

Scheme detection splits on **`://`, never a bare `:`** — `C:/Users/…` is a
Windows path.

### Failures name their step

`StagingError` carries a stable `code`, a `step` and a `message`, and
`user_message()` is `"<step>: <message>"`, so an unattributed staging failure is
not representable. Codes: `no_file`, `not_found`, `unsupported_scheme`,
`insufficient_space`, `staging_dir`, `provider_open_failed`,
`provider_read_failed`, `write_failed`, `short_write`, `empty_read`, `cancelled`.

**A zero-byte read is `empty_read`, and the empty file is deleted.** Leaving it
would be a path to a 0-byte "archive" that fails much later as corrupt. Every
failure path removes the partial copy.

### Free space

`ensure_free_space()` measures the **per-feature staging folder actually used**,
walking up to the nearest existing ancestor (`statvfs` needs a real path and the
folder may not exist yet — same volume either way). It requires the file plus a
32 MB margin. Two deliberate non-failures: an **unknown** size (a provider that
reports no `_size`) skips the check, since an unmeasurable file is not evidence
of a full disk; and an unreadable `statvfs` logs and continues rather than
refusing an import over a missing figure.

## 4. The Android picker fallback

On the reporting ChromeOS device, **Qt's `FileDialog` returned an empty URL**
while a bare `ACTION_OPEN_DOCUMENT` intent worked perfectly. Comparing Qt's
`qandroidplatformfiledialoghelper.cpp` against `cpp/android_raw_pick.cpp` leaves
four deltas, the **MIME filter being the leading suspect**: Qt maps `nameFilters`
to `setType()` + `EXTRA_MIME_TYPES`, and the diagnostic that worked set `*/*`
with no extras.

So the import does not wait for another round trip to find out. It **tries Qt's
dialog and falls back**, and *the fallback firing is the measurement*:

1. `nameFilters` is dropped **on Android only** — gated on
   `Qt.platform.os === "android"`, not `is_mobile` (iOS has neither the defect
   nor the fallback). An **empty array** is the correct "no filter" value: Qt
   tests `if (!nameFilters.isEmpty())` before calling `setMimeTypes()`, so `[]`
   yields `setType("*/*")` and no extras — exactly the diagnostic's
   configuration. The `title` is kept, because with no filter the picker lists
   every file and the title is the only thing left saying what is wanted.
2. `handle_picked_url()` guards the empty URL. **Every** pick goes through it, so
   `scan_source` can never be called with `""` — which is what produced
   `Path not found: ` with nothing after the colon for a whole release. Its
   message (*"The file chooser did not return a file."*) is deliberately
   different from *"Could not access the selected file."*, which means a file was
   named and could not be read.
3. On Android the guard hands over to the raw `ACTION_OPEN_DOCUMENT` picker
   (request code **51305**, never Qt's `1305`) — but **only after telling the
   user**, in one sentence, via a `Dialog` whose `onAccepted` starts the second
   pick. A second picker appearing unannounced reads as a bug. Cancel returns to
   the source frame with the message from step 2.

**The recovered URI is staged as a string, never re-wrapped in a `QUrl`**
(`stage_picked_uri`, sharing the whole worker half with `stage_picked_file`
through `spawn_staging`). `QUrl(uri.toString())` is the conversion under
suspicion; routing the picker's own string back through it would put it straight
back on the path it was recovered from.

**One global slot, one discriminator.** The raw-pick result is delivered through
a process-global `RAW_PICK_TARGET` shared with the About dialog's diagnostic
button. `RawPickConsumer` (`FileSelectionTest` / `DictionaryImport`) is stored
**with** the thread handle, not beside it, so the consumer cannot be read without
the handle it belongs to and a stale consumer from a previous run is not
representable. There is no second parallel mechanism and no polling, and the
cancelled path still completes — an unanswered pick would leave the dialog on the
copying frame forever.

**The private-Qt dependency was promoted deliberately.** `cpp/android_raw_pick.cpp`
is the only file including `QtCore/private/qandroidextras_p.h`, and phase 1 had
confined it to the diagnostic precisely so a shipping feature could not be taken
down by a build break at the next Qt upgrade. Shipping it on the import path is a
decision recorded in the PRD's §11 Q0a; the include stays in that one file, so
the blast radius is unchanged and the failure mode is a **compile error at
upgrade time** on ~80 lines of `#ifdef`-gated code. It can be removed again if
the returned log shows the fallback never firing once the filter is gone.

### `DICTIONARY-IMPORT-PICK:` — one report shape, two callers

The real import path logs the same block as the diagnostic, through the same
`picker_url.rs` pipeline — never a second report shape. `PickReport`
(`Diagnostic` / `DictionaryImport`) decides two things and nothing else: **the
log prefix**, and **whether the block may read the document**.

- The free `line()` helper is a `Block { out, prefix }` struct, so a line written
  without its prefix is not expressible. Passing a prefix argument to every call
  is one forgotten argument away from a block that greps as the wrong feature.
- The import block does **not** perform the diagnostic's 4 MB provider read.
  Staging is about to read the whole file for real. It prints
  `provider_read: (not read here: …)` — a stated measurement rather than a
  missing field.
- Every block states its picker **and** its filter (`filter_config`), and the
  three configurations are three separate literals: the diagnostic's, the raw
  intent's (`RAW_INTENT_FILTER_CONFIG`) and the import dialog's `filter_config`
  property. Nothing derives its configuration from anything else, so changing the
  import dialog's filter cannot silently move what the diagnostic measures.
- Logging runs on a spawned thread and returns nothing the caller acts on (the
  block collects a directory census and a `statvfs`, so it must not run on the
  GUI thread). Desktop behaviour is byte-identical in effect.

`outcome_line()` takes the **probe result**, not just the input. It used to be a
function of the input alone and could not see whether the read had worked — and
on Android every successful pick is `PickerBranch::Provider` while the only
cheerful arm (`LocalFile`) is unreachable there, so **no Android user could see a
line that sounded like it went well.** The user reported the success message as
"the error". The `Provider` arm now has four outcomes: read succeeded (naming the
file and size), opened but read nothing, could not open, and no read attempted.

## 5. The probe extracts nothing

**The archive used to be extracted twice** — once by the probe to read the `.ifo`
and the entry count, once by the import. For a 200 MB archive that is the whole
extraction cost paid twice in wall-clock time, on top of the staged copy.

The probe now reads **two things and nothing else**: the zip's central directory
(`ZipArchive::file_names()` — entry names, zero decompression) and the single
`.ifo` entry, a few hundred bytes of `key=value` text written to a small temp
folder because the `stardict` crate parses from a filesystem path only. That temp
folder holds one text file, not an archive.

Two answered questions behind that route:

- **`stardict::no_cache` requires the `.dict`/`.dict.dz` to be present**
  (`stardict-0.2.3/src/lib.rs`, `get_sub_file("dict", "dz")` →
  `Error::NoFileFound`). That is the bulk of the archive, so "selectively extract
  the `.idx`" was never cheap. The `.ifo`-only route is the only acceptable one.
- **The displayed entry count is the `.ifo`'s declared `wordcount`**, not
  `dict.idx.items.len()`. It is required by the StarDict spec, it is only ever
  *displayed* (the import counts what it actually inserts), and the alternative
  costs a full extraction. `probe_dir_candidate` was moved onto the same read, so
  a dictionary and its own extracted folder now report the **same** number, which
  they did not before.

### A bundle `.zip` is N dictionaries, not one

`-gd` releases are routinely one zip with a folder per dictionary — the reporting
user's `all-dictionaries-gd.zip` is one. The old code reported exactly **one**
candidate: whichever `.ifo` it met first. Every other dictionary in the archive
was unreachable.

Worse, once the probe stopped extracting, the probe and the import **disagreed
about which one**: the probe read the zip's central directory, and
`import_user_zip` extracted everything and took the first `.ifo` the *filesystem*
enumerated. Those two orders are unrelated, so the checklist could offer
dictionary A's title and word count while the import inserted dictionary B —
under the label the user typed for A. Before the probe was made cheap both went
through `locate_stardict_dir` on the extracted tree and agreed by construction.

The fix:

- `probe_zip_candidates()` (plural) returns one `ProbeOutcome` per member;
  `CandidateMeta` carries the `member` folder; `import_user_zip_member()`
  extracts **only that member's entries**.
- **The member is decided once, by the probe, and handed back at import time.**
  Re-deriving it is the defect. The value travels `scan_source` → `scanFinished`
  JSON → `DictionaryImportRow.source_member` → the batch item →
  `import_zip(path, member, label, lang)`.
- **Importing all N members costs one archive's worth of extraction**, not N,
  because each import extracts its own folder only. The existing sequential batch
  driver needed no change.
- **A single-dictionary archive is untouched**: `member` is `None`, the whole
  archive is extracted as before, and the label comes from the zip's filename.
  Only a bundle gets per-member labels taken from the member folder (the zip
  filename is shared, so it would make every row a duplicate). The row's subtitle
  names the member, since two rows of a bundle share a `source_path` and nothing
  else would tell them apart.
- **Order is lexicographic, both between and within folders.** A folder holding
  two `.ifo` files was resolved by "the first one", which meant central-directory
  order in `stardict_members_in` and `read_dir` order in `find_ifo_stem_in` —
  neither specified, and not each other. Both now take the lexicographically
  smallest name.
- **An empty member means "the whole archive".** A dictionary loose at a
  bundle's root has its resources one level down in `res/`, so filtering to
  root-*level* entries would have lost them. The probe reports `None` rather than
  `Some("")` for such a member, so the case is not representable end to end;
  `locate_stardict_dir` looks at the root before any subfolder, so the right
  dictionary is still imported.

**Still true, and worth saying to users:** a bundle is imported one dictionary at
a time and every row becomes its own `dictionaries` row. A list of a dozen rows
where the user expected one reads as a fault unless it is explained.

### Extraction is cancellable and traversal-guarded

`extract_archive()` replaces `ZipArchive::extract`: entry by entry, checking the
`cancel: &AtomicBool` between entries, emitting
`StardictImportProgress::Extracting { done, total }` so the existing progress
frame is determinate. (The single pre-open tick still carries `0, 0`, which QML
already renders as indeterminate.)

Every entry goes through `ZipFile::enclosed_name()`; anything it refuses is
skipped and logged. Symlink entries are written as ordinary files rather than
recreated — strictly the safer of the two, and a StarDict archive has no symlinks
to honour.

> **The traversal test nearly went vacuous, and the guard against that is worth
> keeping.** The first version built the crafted archive with
> `ZipWriter::start_file`, which **normalizes the name** (`options.normalize()`,
> `zip-2.4.2/src/write.rs`) — `../escaped.txt` is stored as `escaped.txt`, so the
> test passed without ever testing traversal. It only surfaced because an
> assertion that the archive really contained a `..` entry was added, and failed.
> The test now writes local headers, central directory and EOCD **by hand**
> (`zip_with_raw_names`) so the hostile name reaches the central directory
> verbatim, and keeps that assertion.

A cancel during extraction now fires **before** any `dictionaries` row exists, so
there is no 0-entry row to clean up: the importer reports `dictionary_id: -1` and
the bridge's empty-abort branch skips the delete rather than asking to remove a
row that was never created.

## 6. What a scan refuses, and what it says

`probe_zip_candidate` used to return `None` for every failure — non-StarDict, bad
zip, full disk, extraction error — and `scan_source` returned `Ok(vec![])`, which
the dialog rendered as *"No StarDict dictionaries were found in the chosen
source."* One sentence for four different problems.

Now:

- `ProbeOutcome` has four variants: `StarDict`, `UnsupportedFormat(ArchiveFormat)`,
  `Unreadable(String)`, `IoFailure(String)`.
- `scan_source` returns a **`ScanReport { candidates, rejections }`** — a struct,
  **not** an enum, because a folder scan legitimately produces both at once
  (three StarDict archives and one MDict).
- Each `ScanRejection` carries a stable `reason` (`unsupported_format` /
  `unreadable` / `io_failure`), an optional `format`, and one plain sentence.
  **Key QML off `reason`, never off the message text.**
- `detect_archive_format()` names the format found by entry name: **MDict**
  (`.mdx`, `.mdd`), **DSL** (`.dsl`, `.dsl.dz`), **XDXF** (`.xdxf`), else "a zip
  of something else". MDict *reading* is out of scope — this is naming only, so
  the user can tell two archives apart. It was written because the reporting user
  had a StarDict archive and an MDict archive side by side and alternated between
  them across five attempts trying to find "the one that would not import".
- Rejections render on **both** surfaces: as the message on the source frame when
  the scan found nothing, **and** as a "N items were skipped:" block on the
  checklist frame when it found some and refused others. Rendering them only on
  the empty path is the same silence the typed report exists to remove.
  The five-line cap on that block is load-bearing, not tidiness: it sits above
  the checklist's `Layout.fillHeight` ScrollView, so every line it prints is a
  line taken from the dictionaries the user came to tick. The remainder collapses
  to "and N more (see the log file for the full list)", and every rejection is
  logged individually.
- **`scan_source` rejects a string that still carries a URL scheme** with its own
  message. A safety net; after the picker work it should be unreachable.
- The UI says **"StarDict/GoldenDict"**, never "StarDict" alone — many users know
  the format only by the GoldenDict name.

## 7. Who owns which temporary file

Three kinds of temporary, three owners, and each has a sweep because a killed
process has no `Drop`.

| Temporary | Where | Deleted by |
|---|---|---|
| the staged copy of the picked archive | `<temp>/simsapa-imports/dictionaries/` | the dialog / `DictionariesWindow.finish_batch()`, via `cleanup_staged_file`; plus `sweep_orphaned_staged_files()` at startup |
| the probe's `.ifo` scratch folder | `SIMSAPA_DIR`, prefix `simsapa-stardict-probe-` | `TempDir` on drop; plus `sweep_orphaned_extract_dirs()` |
| the import's extraction folder | `SIMSAPA_DIR`, prefix `simsapa-stardict-` | `TempDir` on drop; plus `sweep_orphaned_extract_dirs()` |

**Staging is per-feature** (`<temp>/simsapa-imports/dictionaries/`), and
`cleanup_staged_file(path, feature)` **decides ownership by location, not by the
caller's word** — a path outside that folder is refused, so a desktop pick (the
user's own archive, never copied) can never be deleted whatever QML passes in.

The dialog holds `staged_path` and discards it on **every** exit: Cancel from
either frame, an abandoned scan, a scan that found nothing, a scan that failed,
re-entry to `start()`, staging a *second* file in one session (which otherwise
overwrote the property and stranded the first archive), and `onClosing` — the
window-manager close button and Android's back gesture are exits that no Cancel
handler covers. On Import it **clears** `staged_path`, handing ownership to
`DictionariesWindow.finish_batch()`, which every ending goes through: success,
per-item failure and abort alike.

**The feature name is one constant** (`import_staging::DICTIONARY_FEATURE`).
Staging, `cleanup_staged_file` and `sweep_orphaned_staged_files` all key on it,
and a mismatch fails **silently in the worst way**: ownership is decided by
location, so a cleanup pointed at the wrong folder refuses every delete without
an error, and the sweep watches a folder nothing writes to.

Both sweeps run from `init_app_data()` on a background thread, are **age-gated at
one hour**, and treat an **unreadable timestamp as "too young"** — leaving a
stale folder for one more launch is much better than deleting a running import's
extraction directory underneath it.

### `delete_temp_import_folder` is not part of this path

It wipes the **shared root**, not a per-feature subfolder, and is reached only
from `DocumentImportDialog`. The PRD's Req. 19 asked for its signature to take
the feature; that was **deliberately not done**, because that dialog still stages
through the C++ writer into the shared root, so giving it a `"documents"`
argument would point it at a subfolder nothing writes to and silently turn the
one cleanup that does exist into a no-op. The requirement's *intent* — never wipe
the shared root out from under another feature — is met by the dictionary path
not using it at all.

Two things about it that keep being re-derived, and are recorded in a comment on
the function itself: the shared-root behaviour above, and that **Req. 20 is a
non-issue** — `std::env::temp_dir()` and `QStandardPaths::TempLocation` were
measured **identical** on an Android 16 phone and on ARC
(`staging_roots_differ: no`). Do not "fix" it.

## 8. Keep-screen-on holders

**Three holders, not one**, because the three stages are independently long and
can end independently:

| Holder | Acquired | Released |
|---|---|---|
| `dictionary-import-staging` | `begin_staging` | `onStagingFinished` **and** `onStagingFailed` |
| `dictionary-import-scan` | `begin_scan` | `onScanFinished` **and** `onScanFailed` — including when the user abandoned the scan, since the hold must outlive the abandonment |
| `dictionary-import-batch` | `DictionariesWindow.start_batch` | `finish_batch`, which every ending goes through |

Before this work `set_keep_screen_on` was called **nowhere** in the dictionary
flow — a standing CLAUDE.md violation that let the device suspend mid-import.
Neither window had an `AssetManager`; both now have one named `screen_manager`,
used for nothing else. Never release in a dialog's `onClosed`: the worker
outlives the dialog.

## 9. Two cancels, and only one of them is real

- The **copying** frame's Cancel is a true cancel: `abort_staging()` sets an
  `AtomicBool` that the copy checks between chunks; the worker deletes the
  partial file and reports `cancelled`. It travels the same `stagingFailed`
  channel as a real failure — one outcome path, not two — and the dialog tells
  them apart with a `staging_cancelled` flag it set itself, **never** by matching
  the message text.
- The **scanning** frame's Cancel *abandons* rather than cancels: `scan_source`
  has no cancel flag, so the worker runs to completion and its result is
  discarded. That is said plainly in the code comment rather than dressed up.
  Now that the probe no longer extracts, the expensive half of a scan is gone, so
  a real cancel there buys much less than it would have.

## 10. `ANALYZE` — already correct, do not add another

`dictionary_manager_core.rs` runs `ANALYZE` after a successful import and after a
delete. That is the whole requirement. See
[user-data-and-sqlite-analyze.md](./user-data-and-sqlite-analyze.md).

## 11. The Android compile hole

`backend/src/android_saf.rs` is `#[cfg(target_os = "android")]`, so **no desktop
compile and no test in this repo ever sees it** — and staging added ~200 lines to
it (`document_metadata`, `copy_document_to_path`, `parse_uri`). The cross-check
is the only check that file gets:

```sh
cargo check --lib --target aarch64-linux-android
```

with the `CC_`/`CXX_`/`AR_` environment the recipe in the picker task list's
§Notes sets (NDK `27.3.13750724`; ~75 s). **Re-run it after any edit to
`android_saf.rs`.** The same hole applies to `#ifdef Q_OS_ANDROID` C++.
