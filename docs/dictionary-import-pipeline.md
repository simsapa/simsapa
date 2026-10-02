# Dictionary import pipeline

How a user-supplied StarDict/GoldenDict dictionary gets from a file picker into
`dictionaries.sqlite3`, which stage runs on which thread, who owns each
temporary file, and the traps that are easy to reintroduce.

§1–§11 follow a file the user picked. §12 covers the other way in: the
**Available** section, which downloads a curated dictionary and hands it to the
same import stage.

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
where the user expected one reads as a fault unless it is explained — so the
source-selection screen explains it, rather than leaving it to a covering
e-mail. The `.zip` option reads *"A .zip archive — one dictionary, or a bundle of
several"* and carries a note defining a bundle (one `.zip` holding several
dictionaries, each in its own `.zip` file **or** folder) and saying what follows:
every dictionary listed, each imported separately under its own name. Calling
that option "a *single dictionary* .zip archive", as it used to, tells the user
a bundle is the wrong choice on the one screen where it is the only choice.

### A bundle has **two** shapes, and the second one is a zip of zips

The other half of the same defect: `all-dictionaries-gd.zip` — the archive the
report was actually about — does not hold a folder per dictionary. It holds a
**`.zip` per dictionary** (14 of them, all `Stored`), and a probe that only
looked for `.ifo` entries rejected the whole thing with *"does not contain a
StarDict/GoldenDict dictionary"* while `cone-gd.zip` from the same release
imported fine. Both shapes are now read, on desktop and Android alike.

- `nested_zip_entries_in()` lists `.zip` entries by the **same two-level rule**
  `is_shallow_ifo_entry` uses — root or one folder deep. Whatever the scan
  offers, the import must be able to reach. A trailing `/` marks a **directory**
  entry and is excluded: a *folder* named `foo.zip` is a legal bundle member,
  and reading it as a nested archive would file a bogus "could not be read"
  rejection beside the good candidate the folder already produced.
- **A nested archive is opened in place, not copied out.** `FileSlice` is a
  `Read + Seek` view of one byte range of the outer file, so a nested entry
  stored uncompressed *is* already a contiguous run of bytes to hand to
  `ZipArchive::new`. Its length is the entry's **`compressed_size`** — equal to
  `size` for a stored entry by definition, and the one that means "bytes
  actually on disk", so a malformed header cannot run the slice off the end.
  Probing all 14 dictionaries of the 180 MB bundle takes **7 ms** and writes 14
  `.ifo` files. `open_nested_archive()` falls back to copying the entry out only
  when it is **deflated** (a deflate stream cannot be seeked); all 14 members of
  the one bundle measured are `Stored`, because deflating an already-compressed
  zip gains nothing.
- **A copy, when one is needed, is deleted the moment its archive is done
  with** — `NestedArchive::discard()`, archive handle dropped *before* the file
  (Windows will not delete an open file). Holding them to the end of the scan
  would put the whole bundle in temp files at once, which on the archive this
  feature exists for is 180 MB on a phone. Two known limits of that fallback,
  both accepted rather than fixed because no measured bundle is deflated: the
  copy is **not cancellable and reports no progress**, so a cancel waits for one
  `std::io::copy`; and the import's copy lives inside the extraction temp dir,
  where `locate_stardict_dir` cannot mistake a `.zip` for a dictionary.
- **The member string carries the nested entry**, jar-style:
  `"abt.zip!/"` for the usual one-dictionary-per-zip case, `"abt.zip!/pts"` when
  a nested archive itself holds several. The separator is always present, so
  `split_nested_member()` never has to guess from the `.zip` extension — a
  *folder* named `whatever.zip` is a legal bundle member and must not be
  mistaken for a nested archive. Splitting is on the **last** `!/`, which is
  exact because a folder member never contains a `/`. `DictionaryImportRow`'s
  `member_display` renders it back as `abt.zip` / `abt.zip / pts`; the separator
  is for the import, not for the eye.
- **A nested member's `member` is always `Some`**, bundle or not — unlike a
  folder member, it cannot be reached by extracting the outer archive, which
  yields `.zip` files and no `.ifo`.
- **The label comes from the nested archive's own filename** (`abt.zip` → `abt`),
  which is the only thing telling two rows of one bundle apart.
- **A nested archive that is not a dictionary is rejected by name** — *"…
  contains "mdict.zip", which is an MDict dictionary (.mdx)."* — never as the
  archive, or one bad member among twelve good ones reads as "this archive is
  not a dictionary".

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
| the staged copy of the picked archive, or an archive downloaded from the Available list (§12) | `<temp>/simsapa-imports/dictionaries/` | the dialog / `DictionariesWindow.finish_batch()`, via `cleanup_staged_file`; plus `sweep_orphaned_staged_files()` at startup |
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

**Four holders, not one**, because the stages are independently long and can end
independently:

| Holder | Acquired | Released |
|---|---|---|
| `dictionary-import-staging` | `begin_staging` | `onStagingFinished` **and** `onStagingFailed` |
| `dictionary-import-scan` | `begin_scan` | `onScanFinished` **and** `onScanFailed` — including when the user abandoned the scan, since the hold must outlive the abandonment |
| `dictionary-import-batch` | `DictionariesWindow.start_batch` | `finish_batch`, which every ending goes through |
| `dictionary-download-batch` | `DictionariesWindow.start_download_run` (§12) | `finish_download_phase`, which every ending goes through — **after** it has called `start_batch`, so the two holds overlap rather than leave a gap |

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

## 12. Available dictionaries — a second entry point into the same import

The **Available** section of the Dictionaries window offers ten curated
dictionaries from `digitalpalidictionary/other-dictionaries` with a checkbox each
and one **Download and Import** button. It is a second way *into* the pipeline
above, not a second pipeline: it replaces **pick → stage → probe → choose** with
**resolve → download**, and joins the existing batch at **import**.

```
resolve tag  →  download (into the staging folder)  →  import   ← the §1 import stage, unchanged
```

| Stage | Runs on | Entry point | Reports through |
|---|---|---|---|
| **resolve** | worker thread | `DictionaryManager::refresh_available_dictionaries()` → `dictionary_catalog::resolve_catalogue()` | `availableDictionariesReady(items_json)` |
| **download** | one worker thread for the whole run, one item at a time | `DictionaryManager::download_available(labels)` → `dictionary_catalog_download::download_entry()` | `availableDownloadProgress` / `availableDownloadFinished` / `availableDownloadFailed` |
| **import** | the existing batch | `DictionariesWindow.start_batch(items)` → `import_zip(path, "", label, lang)` | the §1 import signals |

**`scan_source` is never called.** Each downloaded archive enters the batch as
`{ kind: "zip", path, member: "", label, lang }` — `member` empty because every
catalogue archive holds exactly one dictionary at its root (§5: an empty member
means the whole archive), `label` and `lang` from the catalogue. Nothing is
probed and the user is asked nothing; that the answers are already known is the
feature.

The code is split so the pure half stays offline-testable:
`backend/src/dictionary_catalog.rs` (the table, the constants, release-list
parsing, tag selection, URL construction — no test touches the network) and
`backend/src/dictionary_catalog_download.rs` (the one function that does).

### The catalogue

`CATALOGUE` is a `const` array of ten `CatalogueEntry`. The array order is the
display order *and* the download order.

- **`label` and `asset_stem` are two fields.** `label` is the import label, the
  uid prefix of every word in the dictionary, and the row's identity in the UI.
  `asset_stem` is the upstream file's basename: the published file is
  `<asset_stem>-gd.zip`. They differ for one entry — `nyana`, whose asset is
  `nyanatiloka-gd.zip` — so anything that names the *file* (the download URL, the
  match against the API's asset list, the staged file name, the 404 message) must
  use `asset_stem`, and anything that names the *dictionary* (the signals, the
  batch item, the hide rule) must use `label`.
- **`abt` is the CPED, and its `name` is deliberately not its `.ifo`
  `bookname`.** Every other `name` is the archive's own `bookname`. `abt`'s
  reads "Concise Pali English Dictionary (pi-en)"; the catalogue shows "Ancient
  Buddhist Texts Glossary (CPED)", which matches the asset's label and what a
  user looking for the CPED will recognise. There is no `cped` asset.
- **`lang` is stated, never derived from `name`.** `peu`'s `bookname` ends
  `(pa-en)`, and `pa` is ISO 639-1 for Punjabi; upstream means Pali, and the
  entry's `lang` is `pli`.
- **`si` is not a known tokenizer language.** `KNOWN_TOKENIZER_LANGS`
  (`bridges/src/dictionary_manager.rs`) has no `si`, so `sin-eng-sin` is indexed
  with the default tokenizer — exactly what a manual import with `si` gets. The
  import succeeds; this is not a fault to work around.
- **Five upstream assets are excluded on purpose**: `all-dictionaries` (a bundle
  of everything), `bold-def`, `dppn`, `dpr` and `simsapa` (shipped with the app,
  or duplicating what is). Only `-gd.zip` assets are used; the `-mdict.zip`
  twins are a format the importer does not read. A test asserts none of the five
  is in the table.

### Which release — the pinned series and the fallback tag

**This is not the app's own release lookup.**
[releases-info-and-fallback.md](./releases-info-and-fallback.md) describes
Simsapa's releases feed (the pythonanywhere endpoint plus
`assets/releases-fallback.json`), keyed on the app's own version and database
version. `other-dictionaries` is a third-party GitHub repository that does not
appear in that feed. The two share `update_checker::to_version()` and
`compare_versions()` and nothing else — do not route this through
`compatible_assets_release()`.

`resolve_catalogue()` asks
`https://api.github.com/repos/<REPO>/releases` and `select_tag()` picks the
**highest tag whose major and minor equal `PINNED_SERIES`** (`(1, 0)`), skipping
drafts, prereleases and tags that do not parse. `to_version()` strips a leading
`v` itself, so tags are passed to it as published.

- **A release outside the series is ignored, not downloaded.** A `v1.1.0` or
  `v2.0.0` upstream changes nothing until `PINNED_SERIES` is edited. **Raising
  the pin is a code change and must not become a setting**: a minor or major
  bump is exactly when the import itself is most likely to need work — renamed
  assets, a different archive layout, a new format — and the pin is the one gate
  that makes someone compare the new assets against the catalogue first.
- **Every failure resolves to `FALLBACK_TAG`** (`v1.0.8`) — offline, DNS, an
  HTTP error, the rate limit, unparseable JSON, or a list with no tag in the
  series. The list still renders; nothing is shown to a user who is only
  browsing. The failure is logged, followed by one line per resolution:
  `dictionary_catalog: resolved tag v1.0.8 (source: fallback)`.
- **URLs and sizes follow the source.** From the API they are the asset's own
  `browser_download_url` and `size`. Under the fallback the URL is constructed
  (`…/releases/download/<tag>/<asset_stem>-gd.zip`) and the size is the entry's
  baked-in `fallback_size_bytes`, flagged `size_is_approximate` so the UI
  prefixes it with `~`. A catalogue entry missing from an otherwise successful
  API response is resolved the fallback way on its own.
- **Only a successful resolution is cached** (process lifetime), so reopening
  the window does not spend another of GitHub's 60 unauthenticated requests per
  hour. A fallback is never cached: reopening the window is how a user retries,
  and a cached fallback would outlive the outage that caused it.
- **The `User-Agent` header is required.** `api.github.com` answers 403 to a
  request without one. That is also why a bare 403 is not logged as the rate
  limit — only a 403 carrying `X-RateLimit-Remaining: 0` is.
- **No pagination.** GitHub returns releases newest-first, 30 to a page, so page
  one holds the highest tag of the series unless upstream publishes more than 30
  releases *above* it.

The resolved repo and tag are shown to the user: the info button beside the
**Available** heading opens a note naming
`github.com/digitalpalidictionary/other-dictionaries` and the tag, with a link to
the releases page for dictionaries outside the curated set. An upstream mismatch
is therefore readable from a screenshot.

### The download

`download_entry()` writes `<asset_stem>-gd.zip` into
`staging_dir(DICTIONARY_FEATURE)` — the **same folder a picked archive is staged
into** — and that placement is the whole of its cleanup design. It inherits the
three staging rules of §7 rather than restating them:

1. **Ownership is by location.** `cleanup_staged_file` deletes a path only if it
   is inside the dictionaries staging folder. A download written anywhere else
   would be refused silently and survive every run.
2. **The feature name is the one constant**, `import_staging::DICTIONARY_FEATURE`.
   The downloader, the cleanup and the startup sweep must agree on it, and a
   mismatch has no error to report.
3. **The startup sweep already covers a crash.** `sweep_orphaned_staged_files`
   reclaims an archive orphaned mid-download on a later launch. No second sweep
   exists for downloads.

The copy itself is `import_staging::copy_stream_to_file()` reading the HTTP
response — the same chunked, cancel-checked writer staging uses, which removes
the partial file on every failure path. Around it:

- Free space is checked **before** the connection is opened.
- The client has a **connect timeout and no overall timeout**: a slow line must
  be allowed to finish a 55 MB archive. Redirects need no configuration —
  reqwest's default policy follows the GitHub → CDN hop.
- The HTTP status is checked before any byte is streamed. **404 is its own code,
  `asset_not_found`**, and its message names the asset file and the resolved tag
  (*"nyanatiloka-gd.zip is not available in the upstream release v1.0.9. The
  upstream assets may have been renamed."*). That is what an upstream rename
  looks like, and it is written to `log.txt` so it can be diagnosed without a
  reproduction. Any other non-2xx is `http_status`.
- A finished body is rejected if it is empty (`reject_empty`) or shorter than the
  **response's own `Content-Length`** (`reject_short`, code
  `truncated_download`), and deleted. `reject_short` never compares against
  `fallback_size_bytes` — that figure is approximate and a newer patch release
  legitimately differs from it. A chunked response has nothing to compare
  against and passes.
- The progress total is `Content-Length`, else the catalogue size, so the bar is
  determinate either way. Ticks are throttled to 100 ms, with one forced tick at
  the end so the bar reaches 100%, and cross into QML as `f64`.

Callers match on `StagingError::code`, never on the message.

### The run, in `DictionariesWindow.qml`

`start_download_run()` → (`onAvailableDownloadFinished` /
`onAvailableDownloadFailed` → `advance_download()`)… → `finish_download_phase()`
→ `start_batch()`.

- **The progress frame is `views_stack` index 6 — appended after the error frame
  (5), not placed beside the other progress frames.** The indices are literals at
  every call site in this file; inserting a frame in the middle would silently
  reroute all of them. (The import *dialog* names its frame indices — §1; this
  window does not.)
- **There is no "run finished" signal.** The worker emits one outcome per label,
  and `advance_download()` ends the phase when
  `pending_import_items.length + download_failed.length` reaches the run's
  length. A cancelled item emits nothing, so a cancel cannot complete that count
  — `cancel_download_run()` drives the ending itself.
- **`finish_download_phase()` is the one function every ending passes through**:
  all downloaded, all failed, cancelled. It hands the queue to `start_batch()`
  **first** and releases `dictionary-download-batch` **second**, because
  `start_batch()` acquires `dictionary-import-batch` and the other order leaves a
  moment with no holder (§8).
- **Download failures have their own list.** `start_batch()` resets
  `batch_failed`, so a failure recorded before the hand-off would be erased if it
  were kept there. The summary frame renders `Import failed (N):` from
  `batch_failed` and `Download failed (N):` from `download_failed` as two blocks
  — the user needs to know whether to check their connection or report a bad
  archive.
- **A run that reaches the import phase ends as `import_batch`.**
  `finish_batch()` sets that op-kind unconditionally and was not given a branch
  for this flow; the `import_batch` summary appends the download failures
  whenever there are any. `download_batch` is the op-kind for the one ending that
  never reaches `start_batch()`: nothing was downloaded.
- **A failure does not stop the run**, in either phase. The failed dictionary
  was never imported, so it is still listed under Available, and it comes back
  **unchecked** — `finish_download_phase()` drops the whole run's labels from the
  selection.
- **Cancel** during the download phase sets `download_cancel`, which the worker
  checks between chunks and between items; archives already downloaded still
  import. Once the import phase has started, the import frame's Abort and the
  existing `abort_import()` take over. `download_cancel` is a third flag beside
  `staging_cancel` and `import_cancel` because it belongs to a third screen.
- **A download that completes inside the cancel window is deleted on arrival.**
  The run has already ended, so that archive will never enter the batch queue and
  `finish_batch()` will never clean it up; `onAvailableDownloadFinished` calls
  `cleanup_staged_file(path)` itself when `download_active` is false.
- **The window refuses to close on frame 6**, as it does on 1, 2 and 3. That
  refusal is what lets this window be destroyed on close without a deferred-
  destruction path (see
  [window-lifecycle-and-reuse.md](./window-lifecycle-and-reuse.md) §5b).

### The hide rule is a filter, and the selection must go through it

`available_filtered()` drops every catalogue entry whose `label` matches an
imported dictionary's. It is computed from `user_dictionaries`, which
`refresh_list()` already rewrites after every import, delete and rename — so a
row disappears when its import finishes and reappears when the dictionary is
deleted, with no refresh path of its own.

- **Read the selection through `checked_offered()`, never `checked_labels`.**
  The raw list can hold a label the filter has since removed. Counting it claims
  "1 selected" with no checkbox on screen, and running it downloads a dictionary
  again only for `import_zip` to refuse the duplicate label. The count, the
  combined size, the button's enabled state and the run's label list all go
  through `checked_offered()`.
- **The match is on label only.** A dictionary imported by hand from the same
  archive took the label its file name suggested (`cone-gd`), so `cone` is still
  offered and the user can end up with both. That is accepted — both are visible
  in the imported list — and no content matching exists.
- **`human_size()` in QML mirrors `import_staging::human_bytes()` exactly.** The
  rows show the backend's `size_text`; the combined-size line and the progress
  frame are formatted in QML. They are on screen together and must agree to the
  digit.
- **Resolution is asynchronous only.** There is no synchronous getter for the
  catalogue: a resolution that is not cached blocks on a GitHub request, so it is
  reachable only through `refresh_available_dictionaries()` and its signal.
