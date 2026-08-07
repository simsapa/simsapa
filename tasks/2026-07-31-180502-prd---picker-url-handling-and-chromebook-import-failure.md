# PRD — Robust file-picker URL handling (fixing the Chromebook "Path not found" import failure)

- **Date:** 2026-07-31
- **Amended:** 2026-08-06 — see §2.1, §2.1a and §4A
- **Status:** Draft — not yet implemented. Split into **two phases**:
  - **Phase 1 — the "File Selection Test" button (§4A).** A diagnostic that
    changes no import behaviour, shipped in the same build as **Run Storage
    Diagnostics** (`tasks/2026-08-05-201545-prd---run-storage-diagnostics.md`)
    and sent to the same user, who reports both. **This is what gets
    implemented first.**
  - **Phase 2 — the fix (§5 onwards).** Blocked on phase 1's data, exactly as
    the fulltext-index fix is blocked on the storage diagnostics.
- **Reported by:** A Chromebook user importing a StarDict `.zip` via the Dictionaries window
- **Same user as the storage report.** `feedback-and-bug-reports/log-chromebook.txt`
  carries *both* failures: the `scan_source` errors at `:127-128` and the
  external-volume storage path at `:132` that produced the fulltext-search PRD.
  One build and one round of email answers both investigations.

## 1. Introduction / Overview

A user on a Chromebook (running the Android build of Simsapa) tried to import a
StarDict dictionary `.zip` through **Dictionaries → Import StarDict → "A single
dictionary .zip archive"**. Instead of a list of discovered dictionaries, the app
showed:

> Scan failed: Path not found: …

The import is impossible for that user. Nothing in the UI explains why, and the
native half of the failure is traced only by `qWarning` into `logcat`, which an
ordinary Chromebook user cannot retrieve.

Investigation traced this to how the app converts a **file-picker URL into
something the Rust backend can open**. That conversion is currently done by
hand-rolled QML string manipulation, duplicated in **four** places, and it has
**five** independent defects (§2.1a, §2.2–§2.5) — most of which bite hardest on
ChromeOS. The same code is used by document import, chanting-data import and the
Gloss "Open JSON" action, so the reported bug is one symptom of a shared root
cause.

**But the user's log shows a failure none of those five rewrites would
necessarily fix** (§2.1): the picker returned an **empty** URL, and why it did
so cannot be determined from here. That is why the work is now split — a
diagnostic first (§4A), the rewrite second.

**Goal:** first, measure what the ChromeOS picker actually returns, on the
affected device, without asking the user technical questions (§4A). Then replace
the duplicated, string-based URL handling with one correct, shared mechanism
that works on ChromeOS, plain Android and desktop, and make any remaining
failure self-explanatory to the user instead of silent (§5 onwards).

## 2. Background — what actually goes wrong

A junior developer should read this section before touching the code; the bugs
are subtle and easy to "fix" incorrectly.

### 2.1 The error can only come from one place — and the path it printed was **empty**

> **Amended 2026-08-06.** This section previously reasoned from the *shape* of
> the path the user's error message would contain. The user's log has since been
> read, and it settles the question in a way the original draft did not
> anticipate. The correction propagates into §2.1a (a new defect), §4A (the
> phase-1 diagnostic) and Appendix A (now superseded).

`"Path not found"` is produced in exactly one location,
`backend/src/dictionary_manager_core.rs:501`:

```rust
match path.try_exists() {
    Ok(true) => {}
    Ok(false) => return Err(format!("Path not found: {}", path.display())),
    Err(e) => return Err(format!("Cannot access {}: {}", path.display(), e)),
}
```

This is reached only when QML passed Rust a string that does not resolve to an
existing filesystem entry.

**The evidence.** `feedback-and-bug-reports/log-chromebook.txt:127-128` records
both of the user's attempts, two minutes apart:

```
[2026-08-03 13:07:07.526Z] ERROR: scan_source failed: Path not found:
[2026-08-03 13:09:25.802Z] ERROR: scan_source failed: Path not found:
```

**Nothing follows the colon.** The line ends there (verified with `cat -A`). The
path was the **empty string** — not a `content://` URI, not an `externalfile:`
URI, not a sandboxed `/storage/emulated/0/…` path.

Tracing that back through the single call chain:

- `scan_source` has exactly one caller, `begin_scan` at
  `assets/qml/DictionaryImportDialog.qml:111`, reached only from `:187` (the
  `FileDialog`) and `:198` (the `FolderDialog`). The folder options are
  `visible: root.is_desktop`, so on the Chromebook only `:187` is reachable.
- Therefore `strip_file_scheme(selectedFile)` at `:175` returned `""`.
  `String(url)` on an empty `QUrl` is `""`, which matches neither the
  `file:///` nor the `file://` prefix, so the fall-through at `:102` returned it
  verbatim → `PathBuf::from("")` → `Path not found: `.
- The `content://` guard at `:179` **did not fire**, so the empty-return check
  at `:181` never ran — which is why the user saw *"Path not found"* and not
  *"Could not access the selected file."*

**What this changes.** The original conclusion — "the QML branch that converts a
picker URL into a real file never ran at all" — holds. Its *reason* does not.
The branch was not bypassed because the scheme was unrecognised (Defect A.1) or
because a sandboxed `file://` path was returned (Defect A.2): **the picker
returned no URL at all.**

**Consequences for the rest of this PRD.**

1. Defects A–D (§2.2–§2.5) are all real code defects, verified by reading the
   code, and the fix in §5 remains the right fix. But **none of them is
   demonstrated by this bug report.** A build that fixed all four could ship and
   still leave this user exactly as broken.
2. A **fifth** defect — the unguarded empty URL — is what the report actually
   shows. See §2.1a.
3. The remaining open questions (Q2: does ChromeOS emit `externalfile:`?
   Appendix A.2: which locations work?) **can no longer be answered from the
   shipped error text**, because the URL never reaches Rust to be printed. That
   is the whole reason for phase 1 (§4A).

### 2.1a Defect E — an empty picker URL is passed straight through

*(Lettered E because it was found last, in the user's log; it is listed first
because it is the only defect this bug report actually demonstrates.)*

`assets/qml/DictionaryImportDialog.qml:175` does not check whether
`selectedFile` is valid before converting it:

```qml
let path = root.strip_file_scheme(selectedFile);
```

An empty or invalid `QUrl` becomes `""`, survives both prefix tests in
`strip_file_scheme`, is returned by the `:102` fall-through, and is handed to
Rust as a path. The user sees `Path not found:` with nothing after it — a
message that names no file and suggests no cause.

The same hole exists at every one of the four call sites in §2.6: none of them
tests the URL for validity.

**Why `selectedFile` was empty — a source-level mechanism, found 2026-08-06.**
Qt 6.9.3's own Android file-dialog helper,
`qtbase/src/plugins/platforms/android/qandroidplatformfiledialoghelper.cpp:44-52`:

```cpp
const QJniObject uri = intent.callObjectMethod("getData", "()Landroid/net/Uri;");
if (uri.isValid()) {
    takePersistableUriPermission(uri);
    m_selectedFile.append(QUrl(uri.toString()));   // QString parse, TolerantMode
    Q_EMIT fileSelected(m_selectedFile.constFirst());
    Q_EMIT currentChanged(m_selectedFile.constFirst());
    Q_EMIT accept();
    return true;
}
```

Qt hands the Java `Uri.toString()` to the **`QUrl(QString)` constructor**. If the
ARC picker's URI does not parse as a valid `QUrl`, the result is an **empty
`QUrl`** — and `accept()` is emitted regardless. QML's `onAccepted` then fires
with an empty `selectedFile`, which is exactly the observed behaviour. The whole
file emits **no `qWarning` at all**, so the loss is silent.

This is sufficient to explain the report, and it makes the first of the earlier
candidates the leading one. All three are kept because none is yet *proven* on
the device:

- **Qt's `QUrl(QString)` conversion rejecting the ARC URI** — now
  source-supported, and directly testable by task 3.12, which reproduces the
  conversion against the raw string;
- the `nameFilters: ["StarDict archives (*.zip)"]` at
  `DictionaryImportDialog.qml:171` becoming a MIME filter the ChromeOS Files app
  answers with a document Qt cannot represent — still live, because the helper
  does set `setType` / `EXTRA_MIME_TYPES` (`:162-167`);
- `accepted` firing on a dismissal in the ARC picker.

**Consequence for the diagnostic (D-8a):** the raw URI is destroyed *inside Qt*
before any app code runs, and `currentFile`, `currentFiles` and `selectedFiles`
are all fed from the same `m_selectedFile` list — so on this path they are all
empty too. **A diagnostic built only on Qt's `FileDialog` can confirm "empty" and
learn nothing more.** Recovering the raw string requires launching
`ACTION_OPEN_DOCUMENT` directly and reading `intent.getData().toString()` before
any `QUrl` exists; see the task list's tasks 3.10-3.14.

Distinguishing these is exactly what the phase-1 diagnostic (§4A) is for. Note
that **fixing the guard is not the same as fixing the bug**: a well-worded
"the file picker returned nothing" message is honest and necessary (Req. D-13),
but the user still cannot import. The cause has to be measured first.

### 2.2 Defect A — the conversion gate is too narrow

`assets/qml/DictionaryImportDialog.qml:179`:

```qml
if (Qt.platform.os === "android" && path.startsWith("content://")) {
```

and the fall-through in `strip_file_scheme` at line 102:

```qml
// content:// (Android SAF) or other scheme — return as-is.
return url_str;
```

Anything that is neither `file://…` nor **exactly** `content://…` is returned
verbatim and shipped to Rust as though it were a path. Two ChromeOS-specific
ways that happens:

1. **A provider scheme that is not `content://`.** ChromeOS exposes its own
   volumes (My files, Google Drive, Play files) into the Android container
   through Chrome's own providers; Drive-backed picks have historically been
   returned as `externalfile:`-style URIs. Such a URL passes straight through →
   `try_exists("externalfile://…")` → `Ok(false)` → *"Path not found:
   externalfile://…"*.
2. **A real `file://` path outside the app sandbox.** If the picker returns
   `file:///storage/emulated/0/Download/x.zip`, `strip_file_scheme` produces a
   genuine-*looking* path. But `android/AndroidManifest.xml` declares **no
   storage permission at all** (only `INTERNET`, `ACCESS_NETWORK_STATE`,
   `RECORD_AUDIO`, `MODIFY_AUDIO_SETTINGS`), and under scoped storage at
   `targetSdkVersion 36` a `stat()` on that path fails for an unprivileged app →
   again `Ok(false)` → the same *"Path not found"*.

Both branches produce an identical, uninformative message, which is why the
report alone cannot distinguish them.

### 2.3 Defect B — `String(url)` percent-decodes and corrupts the URI

`assets/qml/DictionaryImportDialog.qml:175` calls
`strip_file_scheme(selectedFile)`, and line 89 does `String(url)` — i.e.
`QUrl::toString()`.

This is precisely the trap already documented in
[docs/android-file-saving-saf.md](../docs/android-file-saving-saf.md) §"The
`to_encoded()` trap": `toString()` / `toDisplayString()` pretty-**decode**
`%3A`→`:` and `%2F`→`/`, corrupting the URI so Android's `Uri.parse` reads the
wrong document.

This hurts more on a Chromebook than on a phone. A phone's Downloads pick is a
short `…/document/msf%3A1003`, which often survives decoding; a ChromeOS pick
carries a deeply encoded document id (`primary%3ADownload%2Ffoo.zip`, or an
`externalfile%3A…%2F…` payload) that decodes into **extra path segments** and is
unrecoverable. `Uri.parse` in `cpp/utils.cpp:262` then resolves to a different,
nonexistent document, `ContentResolver.query` returns nothing, and
`QFile(content_uri).open()` at `cpp/utils.cpp:310` fails.

**Critical implication for the implementer:** you cannot repair this in QML by
re-encoding the string. Once `%2F` has become `/`, the information that
distinguishes "an encoded slash inside one path segment" from "a segment
separator" is gone. The URL must be handed to the backend as a `QUrl`, and the
backend must call `to_encoded()`. This is exactly what `save_file` already does
(`bridges/src/sutta_bridge.rs:1256`, `:3381`, `:614`) — the read path simply
never got the same treatment.

### 2.4 Defect C — `QFile` can only open `content://`

Even with a correctly-encoded URI, `cpp/utils.cpp:310` reads via
`QFile(content_uri)`, which works only through Qt's
`QAndroidContentFileEngine` — i.e. only for the `content://` scheme. Reading
through `ContentResolver.openInputStream(Uri.parse(uri))` instead handles **any**
provider-backed scheme the ChromeOS picker can return, and removes the need to
scheme-sniff in QML at all.

### 2.5 Defect D — the staging folder is shared and never cleaned

`copy_content_uri_to_temp_file` stages into `<TempLocation>/simsapa-imports`
(`cpp/utils.cpp:299`), and `scan_source` records that temp path as the
candidate's `source_path` for the later import step. Two problems:

1. `DocumentImportDialog.qml:360` and `:376` call
   `SuttaBridge.delete_temp_import_folder()`, which wipes that **entire shared
   folder**. A document import performed between the dictionary scan and the
   dictionary import pulls the staged `.zip` out from under the pending import.
2. The Rust side of that cleanup uses `std::env::temp_dir()`
   (`bridges/src/sutta_bridge.rs:3679`) while the C++ writer uses
   `QStandardPaths::TempLocation`. On Android these are not the same directory,
   so the cleanup is very likely a silent no-op and staged files accumulate
   indefinitely.

### 2.6 The four duplicated call sites

| File | Entry point | URL→string helper |
|---|---|---|
| `assets/qml/DictionaryImportDialog.qml` | `FileDialog.onAccepted` (`:174`), `FolderDialog.onAccepted` (`:198`) | `strip_file_scheme` (`:88`) |
| `assets/qml/DocumentImportDialog.qml` | `FileDialog.onAccepted` (`:63`) | inline copy of the same logic |
| `assets/qml/ChantingPracticeWindow.qml` | `FileDialog.onAccepted` (`:536`) | `file_url_to_path` (`:60`) |
| `assets/qml/GlossTab.qml` | `open_json_session_from_url` (`:846`) | `file_url_to_path` (`:815`) |

All four carry Defects A and B. `DictionaryImportDialog`'s `FolderDialog` branch
(`:198`) has **no** `content://` handling whatsoever — it is currently
desktop-only (`visible: root.is_desktop` on the three folder radio buttons), but
that is an accident of UI gating, not a guarantee.

## 3. Goals

1. A Chromebook user can import a StarDict `.zip` from any location the system
   file picker offers, including ChromeOS volumes such as My files, Downloads,
   Play files and Google Drive.
2. Picker URLs are converted to readable local files by **one shared mechanism**
   used by all four import/open call sites — no duplicated string parsing.
3. Percent-encoding in a picker URI is preserved end-to-end (no lossy
   `toString()` in the path from picker to backend).
4. Any remaining failure produces a **specific, on-screen** explanation naming
   what failed, plus a `Logger` trail sufficient to diagnose from a user's
   screenshot alone.
5. No new Android permission is requested, and
   `android/AndroidManifest.xml`'s permission/feature set is unchanged.
6. Staged temporary import files belong to the feature that created them and are
   actually deleted.

## 4. User Stories

- **As a Chromebook user**, I want to import a StarDict `.zip` I downloaded, so
  that I can look up words in a dictionary that Simsapa does not ship.
- **As a Chromebook user**, when an import cannot proceed, I want the app to tell
  me *why* and what to do instead, rather than showing a path I do not recognise.
- **As a user on any platform**, I want document import, chanting-data import and
  Gloss "Open JSON" to behave the same way as dictionary import when I pick a
  file, so that one working picker means they all work.
- **As a maintainer**, I want one place to fix picker-URL handling, so that a
  future platform quirk does not have to be fixed four times.
- **As a maintainer receiving a bug report from a device I do not have**, I want
  the user's screenshot to identify the failing stage, so that I do not need
  `logcat` access to act on it.

## 4A. Phase 1 — the "File Selection Test" button

**Added 2026-08-06.** This section is what gets built and shipped **first**. It
changes no import behaviour; §5 onwards is phase 2 and stays blocked until this
returns data.

### 4A.1 Why a diagnostic and not just the fix

§2.1 established that the picker returned an empty URL, and §2.1a that we cannot
tell why from here. Three facts follow:

1. **The answer cannot be obtained by email.** The shipped error message prints
   the path it was given, and that path is empty — it carries no scheme, no
   encoding, no filename. Appendix A's differential test (retry from Downloads,
   Play files, Drive) would return the identical empty message from every
   location, distinguishing nothing.
2. **The answer cannot be obtained here.** No Chromebook is available, and
   `Qt.platform.os` is `"android"` on ARC, so an ordinary Android phone does not
   reproduce the picker.
3. **Guessing is expensive.** Defects A–D are four independent rewrites across
   four call sites, one native reader and one backend module. Shipping all of it
   against an unproven diagnosis risks a second round trip with the same user.

This mirrors the split that
`tasks/2026-08-05-201545-prd---run-storage-diagnostics.md` made for the fulltext
bug, and for the same reason: measure on the affected device, then fix.

**It is not throwaway work.** The provider-backed reader written here
(Req. D-8) *is* Req. 8's fix for Defect C, in its final location, and the
scheme-dispatch logic *is* Req. 29's `backend/src/picker_url.rs`. Phase 1 wires
them only into the diagnostic; phase 2 flips them into the four call sites.

### 4A.2 The user flow this is designed around

The build ships with **two** buttons in the About window, and the user is asked
to press them **in this order**:

1. **"File Selection Test"** — opens a file picker. The user selects *the same
   `.zip` that failed to import*. The app imports nothing; it measures what the
   picker returned and writes it all to `log.txt` at INFO.
2. **"Run Storage Diagnostics"** — the existing button. Its report is unrelated
   to the import bug, but it is the *same user* (§ header) and the same round
   trip.
3. The user sends back the storage-diagnostics **summary text** (via its Copy
   button) **and** their **`log.txt`** (via About → log file list → "Copy
   Contents" or "Save As…", `AboutDialog.qml:214-231`).

The ordering is load-bearing: the File Selection Test runs first so that its
INFO lines are already in `log.txt` when the user copies it. A user who copies
the log first and tests afterwards sends a log with nothing in it.

**No new export affordance is needed** and none may be added. The storage
dialog's Copy button and the About dialog's existing per-log-file Copy/Save
actions already cover both halves.

### 4A.3 Functional requirements

Numbered `D-n` so as not to collide with the phase-2 requirements 1–30.

**Placement and behaviour**

- **D-1.** A **"File Selection Test"** button must be added to the bottom button
  column of `assets/qml/AboutDialog.qml` (`:255-291`), which since the storage
  diagnostics work is a full-width `ColumnLayout` — three buttons on a row
  overflowed the window on a phone. Place it **between** "Copy App Info"
  (`:261`) and "Run Storage Diagnostics" (`:273`), so the visual order matches
  the order the user is asked to press them.
- **D-2.** The button must be available on **all platforms**. The desktop
  `file://` branch is the regression surface for phase 2, and a maintainer
  running it locally must be able to see the report shape.
- **D-3.** The picker the button opens carries **no file-type filter**. §2.1a
  names the existing `.zip` filter as a candidate cause of the empty URL; a
  diagnostic that inherits the suspect configuration cannot test it. The test
  must also be runnable against any file, not only a `.zip`.
- **D-3a.** *(Amended 2026-08-07, once Q0a was reversed and the raw-intent
  capture existed.)* **One button press opens exactly one picker**, and which
  picker depends on the platform:
  - **desktop** — Qt's `FileDialog`, with no `nameFilters`;
  - **Android** — the app's own `ACTION_OPEN_DOCUMENT` (`CATEGORY_OPENABLE`,
    `setType("*/*")`), **not** Qt's `FileDialog`.

  The reason is that on Android the two paths do not carry equal information.
  Qt's `FileDialog` can only tell us the URL was empty, which the user's log
  (§2.1) already established; the raw intent additionally yields the picker's
  URI as a string, and the report reproduces Qt's own conversion from it (D-8h).
  Running both would open **two consecutive pickers per press**, contradicting
  Appendix B.2's instruction to pick the file once and inviting the user to
  cancel one of them.

  The cost is accepted and recorded: nothing then exercises Qt's `setType` /
  `EXTRA_MIME_TYPES` as `DictionaryImportDialog` configures it. That only
  becomes interesting if the raw URI turns out to parse cleanly — the last row
  of §4A.5 — and it is a second round trip if so.
- **D-3b.** The raw-intent path must set **no MIME filter** either, for the same
  reason as D-3, and must use a request code that cannot collide with Qt's own
  (`1305`, `qandroidplatformfiledialoghelper.cpp:24`) — the activity result is
  dispatched by request code, and a clash would cross the two dialogs' results
  over.
- **D-4.** The test must **import nothing, stage nothing into the import folders,
  and modify no app data**. It may write its own probe file only if a
  measurement requires one, removed via a `Drop` guard, per the storage PRD's
  FR-39.
- **D-5.** The test must run **off the UI thread** where it does I/O, and must be
  bracketed with `AssetManager.set_keep_screen_on(true)` / `(false)`, released on
  both success and failure, per CLAUDE.md — a Drive-backed pick can stream over
  the network (§9.5).
- **D-6.** The result must also be shown **on screen**, briefly: at minimum a
  one-line outcome and a reminder that the detail is in `log.txt`. The user
  should be able to see that the button did something. A full results window is
  **not** required — `log.txt` is the deliverable.

**What it must measure and log**

- **D-7.** Every line must be written through the Rust logger at **INFO**, so it
  lands in `log.txt` (the file the user sends). Each line must carry a
  distinctive prefix, e.g. `FILE-SELECTION-TEST:`, so the block is greppable and
  the user can be told to look for it if a paste is truncated. QML-side lines use
  `Logger { id: logger }` — already present at `AboutDialog.qml:14` — with a
  single concatenated string argument (CLAUDE.md), never `console`.
- **D-8.** For the selected URL, the report must record, each on its own labelled
  line:
  - a) **whether the URL is empty or invalid** — *this is the first thing
    measured*, since it is the only failure the report actually demonstrates
    (§2.1a). If it is empty, say so explicitly and continue with whatever else
    can still be read (`FileDialog.currentFile`, `selectedFiles.length`,
    `currentFolder` — note these are the Qt 6 property names; `folder` is Qt 5
    and does not exist), rather than stopping at the first blank. **Per §2.1a
    these will all be empty together on the suspected path**, so they corroborate
    rather than recover — the raw URI needs the direct-intent capture;
  - b) the **fully-encoded** form, via `QUrl::to_encoded()` on the Rust side —
    the same call `save_file` already makes (`bridges/src/sutta_bridge.rs:614`,
    `:1256`, `:3381`);
  - c) the **pretty-decoded** form, `toString()`, **printed beside it**. If the
    two differ, that difference *is* Defect B, measured rather than inferred; if
    they are identical, Defect B is ruled out for this pick;
  - d) the **scheme** (answers Q2 directly), the host, and the path segment
    count;
  - e) for `file://`: the local path via **`toLocalFile()`** semantics, and the
    `try_exists()` result for it (Req. 7a — do not use `QUrl::path()`, which
    drops the host);
  - f) for `content://` **or any other provider scheme**: whether
    `ContentResolver.openInputStream(Uri.parse(uri))` opens, the
    `OpenableColumns.DISPLAY_NAME`, the reported size, and the number of bytes
    successfully read from a **capped** read (a few MB is sufficient — the test
    must not copy a 200 MB archive);
  - g) elapsed milliseconds for the provider open and for the capped read —
    §9.5's Drive-streaming concern is a latency question and this is the only
    place it gets measured.
  - h) *(Added 2026-08-07 with the raw-intent capture.)* On the Android
    raw-intent path, the **URI exactly as the picker returned it** — the Java
    `Uri.toString()` string, never round-tripped through a `QUrl` — printed
    beside **whether `QUrl(that string)` is valid**. These two lines together are
    the most valuable in the report: they reproduce
    `qandroidplatformfiledialoghelper.cpp:48` and show directly whether that
    conversion is where the URL is lost. Also record which branch of the result
    produced the string (`getData`, `getClipData`, cancelled, no-URI), and label
    every block with which picker it came from (D-3a), so blocks from different
    platforms are never compared as though they were the same measurement.

    The reproduction must use the **same constructor** Qt uses — the
    `QUrl(QString)` overload, which parses in `TolerantMode`. Verified
    2026-08-07: cxx-qt-lib's `QUrl::from(&QString)` resolves through
    `qurl_init_from_qstring` to exactly that constructor, so the Rust side
    reproduces it faithfully.
- **D-9.** The provider read of D-8(f) must go through
  `ContentResolver.openInputStream`, **not** `QFile(content_uri)`. This is
  Req. 8 (Defect C) implemented in its final form: `QFile` works only for
  `content://` via `QAndroidContentFileEngine`, so using it here would fail on
  precisely the non-`content://` scheme the test exists to detect.
- **D-10.** The scheme-dispatch and URL-normalization decisions must live in the
  Qt-free backend module of Req. 29 (`backend/src/picker_url.rs`), with only the
  JNI I/O behind `#ifdef Q_OS_ANDROID`, so the branch selection is unit-testable
  here without a Chromebook.
- **D-11.** The test must be **repeatable**: pressing the button again runs
  another test and appends another block. The user will be asked to repeat it
  from several locations (Downloads, Play files, Drive), which is Appendix A.2's
  differential test with machine-readable output instead of a screenshot.
  Each block must carry a run counter and a timestamp so the blocks are
  distinguishable in the log.
- **D-12.** The test must record the **import staging facts**, which are pure
  measurement and settle Defect D without any user input:
  - the C++ staging root (`QStandardPaths::TempLocation` + `/simsapa-imports`,
    `cpp/utils.cpp:645`) **and** the Rust one (`std::env::temp_dir()`,
    `bridges/src/sutta_bridge.rs:3687`), printed side by side with an explicit
    "these differ" line when they do. §2.5 currently calls the mismatch "very
    likely a silent no-op"; this turns it into a measured fact;
  - whether the staging folder exists, its file count, total size, and the age of
    its oldest entry — evidence for or against Req. 21a's unbounded-footprint
    claim;
  - free space on the staging volume, via `fs4` (already a direct dependency of
    `backend` since the storage diagnostics work) — this is the input Req. 24's
    threshold needs.
- **D-13.** When the test finds an empty or invalid URL, the on-screen line of
  D-6 must say so in plain words — "the file picker did not return a file" —
  and must not print `Path not found:`. This wording is the model for the
  phase-2 message required by Req. 13.

**Making the native half visible**

- **D-14.** Implement **Req. 17a in phase 1**: install a `qInstallMessageHandler`
  routing `qWarning`/`qCritical` into the app logger. There is currently **no
  message handler anywhere in `cpp/`** (verified 2026-08-06), so all five
  `qWarning`s in `copy_content_uri_to_temp_file` (`cpp/utils.cpp:650`, `:657`,
  `:664`, `:672`, `:684`) go **only to logcat** — unreachable for this user. It
  is a small change, it makes the *existing* release's native failures visible in
  the log the user is already being asked to send, and without it any native
  failure inside D-8(f) lands somewhere the reporting user cannot reach.
  New native code in this phase must use `log_info_c()` / `log_error_c()`
  regardless, per CLAUDE.md — a `qInfo()` on Android is tagged with the
  application name and is filtered out of the documented
  `adb logcat -s simsapa Qt QtCore QtQml` tag set entirely.

### 4A.4 Phase-1 non-goals

- **Fixing the import.** None of the four call sites in §2.6 is migrated in phase
  1. `strip_file_scheme`, `file_url_to_path` and the inline copies stay exactly
  as they are, so the only behaviour change in the build is two new buttons.
  (Req. 15's `scan_source` scheme rejection and the D-13 guard are the only
  wording changes that may ship early, and only if they touch nothing else.)
- **A results window for the file test.** The storage diagnostics has one because
  its report is the deliverable; here the deliverable is `log.txt`.
- **Uploading anything.** The user copies and sends. No network calls.
- **Any new Android permission**, per Req. 26. Phase 1 changes
  `android/AndroidManifest.xml` not at all.

### 4A.5 Decision gate — what each phase-1 outcome means

| D-8(a) URL | D-8(c) encoded vs decoded | D-8(d) scheme | Conclusion for phase 2 |
|---|---|---|---|
| **empty** | — | — | **Defect E confirmed as the user's bug.** The URL→path rewrite is *not* the fix. Investigate the Qt Android `FileDialog` → ARC picker mapping; D-3's unfiltered dialog tells us whether `nameFilters` is implicated. |
| non-empty | **differ** | any | **Defect B confirmed.** Reqs. 2–3 (`QUrl` + `to_encoded()`) are the fix, as drafted. |
| non-empty | identical | not `content://` | **Defect A.1 confirmed**, Q2 answered. Req. 4(c) + D-9's provider reader are the fix. |
| non-empty | identical | `file://`, `try_exists()` false | **Defect A.2 confirmed.** Scoped storage; Req. 14's honest message plus Req. 14a's Downloads workaround is all that is available. |
| non-empty, provider read **succeeds** | identical | `content://` | The pick is fine and the failure is downstream — re-triage from D-12's staging facts and the `scan_source` path. |

**Rows for the Android raw-intent path (D-8h), added 2026-08-07.** Read these
*first* on an Android block: they sit upstream of everything above, because they
report what the picker returned before any `QUrl` existed.

| D-8(h) raw URI | `QUrl(raw)` valid? | Conclusion for phase 2 |
|---|---|---|
| non-empty | **invalid** | **The bug is Qt's `QUrl(QString)` conversion at `qandroidplatformfiledialoghelper.cpp:48`**, exactly as §2.1a's mechanism predicts. None of §5's requirements is the fix. The work becomes: normalize or bypass that conversion for the import path — and since the 6.11 branch still carries the line unchanged (§11 Q0a), an upstream fix cannot be waited for. Compare the raw string against `QUrl`'s parsing rules to find *what* it rejects. |
| non-empty | **valid** | The picker and the conversion are both fine, so the loss is **downstream of Qt's dialog** — the leading remaining suspect is the `nameFilters` → `setType`/`EXTRA_MIME_TYPES` mapping that D-3a deliberately does not exercise. This is the case that earns a second round trip with a Qt-`FileDialog` run. Take the scheme and encoding lines to the table above. |
| **empty**, branch = `cancelled` | — | The user backed out of the picker. Not a finding; ask for another run. |
| **empty**, branch = `no-uri` / `no-intent` | — | The picker reported success but returned no URI at all — a case Qt's helper drops silently. The failure is in the ARC picker or the intent, upstream of every URL question in this PRD. |

## 5. Functional Requirements (phase 2 — blocked on §4A)

### 5.1 Shared URL → local path resolution

1. The system must provide **one** backend function that accepts a file-picker
   URL as a `QUrl` (not a `QString`) and reports either a readable **local
   filesystem path** or an explicit, typed failure. Name it descriptively, e.g.
   `SuttaBridge.resolve_picker_url(url: url, purpose: string): string`, where the
   return value is a **JSON string** (the project's established convention for
   structured bridge results — see `scan_source`, `get_word_json`) of the shape:

   ```json
   { "ok": true,  "path": "/…/simsapa-imports/dictionaries/mw-gd.zip" }
   { "ok": false, "reason": "unsupported_scheme", "detail": "externalfile", "message": "…" }
   ```

   This supersedes any reading of Req. 5 as "returns a bare path string"; there
   is exactly one return channel and it carries the failure reason.
2. That function must accept the QML `url` property (`selectedFile` /
   `selectedFolder`) **directly**, with no intermediate JavaScript string
   conversion at the call site. This pattern is already proven in production —
   `AboutDialog.qml:282`, `GlossTab.qml:748` and `PromptsTab.qml:638` all pass
   `selectedFolder` straight into `save_file(folder_url: &QUrl, …)`.
3. On the Rust side, the URI must be recovered with `folder_url.to_encoded()` (or
   equivalent fully-encoded form), mirroring the existing `save_file` /
   `save_bytes_to_folder` implementation in `bridges/src/sutta_bridge.rs`. It
   must **not** use `toString()`, `toDisplayString()`, or `QUrl::path()`.
4. The function must handle these cases:
   - a) **`file://` URL** → convert to a local path and verify readability.
   - b) **`content://` URL** → materialize to a temp file and return that path.
   - c) **Any other non-empty scheme** (e.g. `externalfile:`) → attempt the same
     provider-backed materialization as (b); only if that fails, report failure.
   - d) **A bare path with no scheme** → treat as a local path.
5. The result must distinguish at least these `reason` values:
   `unsupported_scheme`, `provider_read_failed`, `not_found`,
   `permission_denied`, `insufficient_space`. A bare empty string on failure is
   not sufficient (see Req. 14).
6. All four call sites listed in §2.6 must be migrated to this function, and
   their bespoke `strip_file_scheme` / `file_url_to_path` / inline copies
   removed. This includes `DictionaryImportDialog`'s `FolderDialog` branch.
7. The desktop behaviour must be preserved exactly: `file:///C:/…` on Windows
   yields `C:/…`; `file:///path` on Unix yields `/path`; percent-escapes such as
   `%20` are decoded in the resulting local path.
7a. For the `file://` branch, prefer `QUrl::toLocalFile()` semantics over the
   existing `qurl_to_local_path` helper (`bridges/src/sutta_bridge.rs:584`),
   which is built on `QUrl::path()`. `path()` **drops the host**, so a Windows
   UNC pick (`file://server/share/dict.zip`) collapses to `/share/dict.zip` and
   then fails as "not found". `toLocalFile()` produces `\\server\share\dict.zip`
   correctly. If `qurl_to_local_path` is reused, it must be extended to handle a
   non-empty host; do not silently inherit the defect.

### 5.2 Provider-backed file reading

8. The native materialization step (currently
   `copy_content_uri_to_temp_file` in `cpp/utils.cpp`) must read through
   `ContentResolver.openInputStream(Uri.parse(uri))` rather than
   `QFile(content_uri)`, so that provider schemes other than `content://` work.
9. It must continue to resolve the user-visible filename via
   `OpenableColumns.DISPLAY_NAME`, with the existing fallbacks, and must sanitize
   the result so that a display name containing a path separator cannot escape
   the staging directory.
10. It must copy in **fixed-size chunks** (e.g. 1 MB) rather than `readAll()`
    into a single `QByteArray`. See §5.5a for the sizing rationale — this is a
    robustness and peak-RSS measure, not a fix for a demonstrated OOM.
11. It must distinguish "read produced zero bytes" from "read succeeded", and
    report the former as a failure rather than returning a path to an empty file.
12. Every failure branch must produce a message identifying **which** step failed
    (URI parse, resolver open, temp dir creation, write, short write).

### 5.3 Error reporting and logging

13. When resolution fails, the initiating QML dialog must display an inline
    message that states the failing stage and, where applicable, the offending
    scheme — for example:
    *"Could not read the selected file: the file picker returned an unsupported
    location (scheme: `externalfile`). Try copying the file into Downloads and
    picking it again."*
    The generic *"Could not access the selected file."* is no longer acceptable
    on its own.
14. When the resolved path exists but is unreadable because it lies outside the
    app sandbox, the message must explain that the app can only read files chosen
    through the system picker, and must **not** suggest granting a permission the
    app does not request.
14a. Failure messages **should offer a concrete workaround** where one exists —
    principally: *"Try copying the file into your Downloads folder and choosing
    it again."* Copying to Downloads addresses both the sandboxed-`file://`
    branch and an awkward provider (a Drive-backed or network location becomes an
    ordinary local document once copied), so it is worth stating on both the
    `unsupported_scheme` and `permission_denied` messages. Do **not** attach it
    to `insufficient_space` or `provider_read_failed`, where it is irrelevant or
    actively unhelpful.
15. `scan_source` in `backend/src/dictionary_manager_core.rs` must reject a
    string that still carries a URL scheme with a distinct message (e.g.
    *"Expected a file path but received a URL: …"*) rather than reporting
    *"Path not found"*. This is a safety net; after Req. 6 it should be
    unreachable.
16. Each stage of the picker flow must log via the `Logger` module
    (`logger.info` / `logger.error`, single concatenated string argument — see
    CLAUDE.md): the raw picked URL, the resolved local path, and the failure
    reason. The `console` API must not be used.
17. Log lines must not be so verbose that they leak the full content of a user's
    file paths beyond what is needed to diagnose (the URL and path themselves are
    acceptable; file contents are not).
17a. **Qt/C++ diagnostics must reach the app log file.** There is currently no
    `qInstallMessageHandler` anywhere in `cpp/`, so every `qWarning()` in
    `copy_content_uri_to_temp_file` (`cpp/utils.cpp:305`, `:312`, `:319`, `:331`,
    `:342`) goes **only to logcat** — invisible to any user who cannot run `adb`.
    That is why the native half of this failure has been undiagnosable. Install a
    message handler that routes `qWarning`/`qCritical` into the same logger the
    Rust and QML sides use, so the About dialog's "Copy Contents" carries the
    whole story. Without this, Req. 12's detailed native failure messages are
    written somewhere the reporting user cannot reach.

### 5.3a Progress feedback during staging

17b. Staging must show visible progress. A Drive-backed 200 MB pick on a
    Chromebook streams over the network (§9.5); today the scanning frame is an
    indeterminate bar with no text, so a slow copy is indistinguishable from a
    hang. Show a distinct "Copying file…" state before the "Scanning…" state, and
    where the byte count is known, a determinate percentage.
17c. The staging step must be bracketed with
    `AssetManager.set_keep_screen_on(true)` / `(false)`, released in the
    completion handler on **both** success and failure, per the CLAUDE.md rule
    for long operations. A staged copy interrupted by device suspend is exactly
    the failure this rule exists to prevent.
17d. Staging must not block the UI thread; it belongs on the worker thread that
    already reports through `scan_finished` / `scan_failed`.

### 5.4 Temporary staging

18. Each feature must stage into its **own** subfolder, e.g.
    `<TempLocation>/simsapa-imports/dictionaries/`,
    `…/documents/`, `…/chanting/`, `…/gloss/`.
19. `delete_temp_import_folder` must delete only the calling feature's subfolder,
    not the shared root. Its signature should take the feature/subfolder name.
20. The Rust and C++ sides must agree on the staging root. Resolve the
    `std::env::temp_dir()` vs. `QStandardPaths::TempLocation` mismatch by having
    a single source of truth (preferably a bridge function exposing the C++
    `QStandardPaths::TempLocation` value to Rust, or vice versa) and verify that
    deletion actually removes files on Android.
21. A staged file must survive from the moment `scan_source` records it as a
    candidate's `source_path` until the corresponding import completes or is
    cancelled.

21a. The staged copy must be **deleted once the import completes or is
    cancelled**. Today nothing deletes it: only `DocumentImportDialog` ever calls
    `delete_temp_import_folder`, so every dictionary, chanting and Gloss import
    leaves its staged file behind permanently — and per Defect D the cleanup that
    does exist is probably a no-op anyway. At 10–200 MB per import this is the
    difference between a bounded and an unbounded footprint.

### 5.5 Size and space budget

Typical StarDict dictionaries are **10–20 MB**; exceptionally large ones reach
**~200 MB**. Target devices have roughly 8 GB RAM. This settles two things.

22. **Memory is not the binding constraint.** A 200 MB `QByteArray` is a native
    (not Java-heap) allocation and would succeed on any 8 GB device, so **no
    pre-flight size warning based on RAM is required**. Chunked copying (Req. 10)
    is still specified because it costs nothing to write, keeps peak RSS flat,
    and protects the low-end 2–3 GB Android phones that are not the target but
    are not excluded either.
23. **Disk is the binding constraint, and the current flow is wasteful.** For one
    200 MB archive the peak transient usage is roughly:
    - staged copy of the `.zip`: 200 MB (Req. 18, under `TempLocation`);
    - **scan** extraction — `probe_zip_candidate` extracts the *whole* archive
      into a `tempfile::tempdir_in(simsapa_dir)` just to read the `.ifo` header
      and the index count (`dictionary_manager_core.rs:459-469`);
    - **import** extraction — `import_user_zip` extracts the *same archive again*
      (`dictionary_manager_core.rs:229-240`).

    The two extractions are sequential, not concurrent, so peak ≈ staged copy +
    one extraction ≈ 200 MB + roughly 2× the compressed size. On Android internal
    storage that is a real amount, and the extraction cost is paid twice in
    wall-clock time.
24. Before staging, the resolver must check free space on the staging volume and
    fail with `insufficient_space` (Req. 5) and a clear message if there is not
    room for the file plus a safety margin. Failing early with an explanation
    beats a truncated copy surfacing later as a corrupt archive.
25. **Recommended, not required:** make `probe_zip_candidate` read the `.ifo` and
    index members directly out of the archive **without** a full extraction (the
    `zip` crate can read individual entries by name), eliminating one of the two
    extractions. A pure win for large archives, but not needed for the Chromebook
    fix — split it out if it complicates the main change.

### 5.6 Platform safety

26. No new entry may be added to `android/AndroidManifest.xml`'s
    `<uses-permission>` or `<uses-feature>` lists. (See
    [docs/android-multi-abi-and-chromeos.md](../docs/android-multi-abi-and-chromeos.md)
    — permissions there imply *required* hardware features to Google Play and
    have already filtered this app off Chromebooks once.)
27. All new native code that uses JNI must be `#ifdef Q_OS_ANDROID`-gated so the
    desktop build is unaffected, matching the existing convention in
    `cpp/utils.cpp` and `backend/src/android_saf.rs`.
28. File-existence checks in new Rust code must use `try_exists()`, never
    `.exists()` (CLAUDE.md).
29. The scheme-dispatch and URL-normalization logic must live in **`backend/`**
    as a Qt-free, unit-testable module (e.g. `backend/src/picker_url.rs`), with a
    thin `bridges/` wrapper that unwraps the `QUrl` into its encoded string form
    and calls it. This mirrors how `dictionary_manager_core.rs` (backend, pure)
    relates to `dictionary_manager.rs` (bridge, Qt).
30. Because the resolver now accepts archives from arbitrary providers, confirm
    that the `zip` crate's `archive.extract()` rejects path-traversal entries
    (`../`, absolute paths) at the version pinned in `backend/Cargo.toml`. Recent
    versions sanitize via `enclosed_name`, but this must be verified rather than
    assumed — the extraction target is inside `SIMSAPA_DIR`.

## 6. Non-Goals (Out of Scope)

- **Requesting any storage permission.** The app stays SAF-only. A `file://`
  path outside the sandbox is reported clearly (Req. 14), not made to work.
- **Changing the import UI flow** — the four radio options, the scanning frame
  and the checklist stay as they are. Only the error text and the new "Copying
  file…" progress state (§5.3a) change.
- **Supporting folder imports on mobile.** The three folder options remain
  desktop-only; the `FolderDialog` branch is migrated for correctness, not to
  enable it on Android.
- **Rewriting the StarDict parsing / import logic** in
  `dictionary_manager_core.rs` beyond Req. 15.
- **A new log export / share affordance.** None is needed: **About → log file
  list → "Copy Contents" / "Save As…"** (`AboutDialog.qml:208-224`) already
  exports the log, and `save_file` handles SAF correctly. Req. 17a only routes
  Qt's `qWarning` stream *into* that existing log; it adds no UI.
- **Reworking the write/save path.** `save_file` and `backend/src/android_saf.rs`
  are already correct; they are the *model* for this work, not a target of it.
- **Any change to `dictionaries.sqlite3` schema or migrations.**

## 7. Design Considerations

- **No new UI surfaces.** Error text is rendered in the existing
  `root.scan_message` label on the source-selection frame
  (`DictionaryImportDialog.qml:227-234`), and in the equivalent existing labels
  or dialogs in the other three call sites.
- Error text should be **two sentences at most**: what failed, and what the user
  can do next. Keep the wording plain — the audience is a reader of Pāli suttas,
  not an Android developer. Avoid raw stack traces or JNI terminology on screen;
  put that detail in the `Logger` output.
- Where a scheme name is shown, present it as an aside (e.g. "(scheme:
  `externalfile`)") so the message reads naturally without it.
- QML additions follow the project conventions: `snake_case` functions and ids,
  `Logger { id: logger }` declared in the root element, single-string log
  arguments.

## 8. Technical Considerations

- **Precedent to copy:** `SuttaBridge.save_file(folder_url: &QUrl, …)` and
  `save_bytes_to_folder` (`bridges/src/sutta_bridge.rs:610-627`, `:1256`,
  `:3381`) already do the `QUrl` + `to_encoded()` + scheme-dispatch dance
  correctly for **writing**. This PRD is essentially the read-side mirror of it.
  `backend/src/android_saf.rs` is the reference for JNI `ContentResolver` usage
  and for the `jni = "0.21"` pin.
- **Bridge registration:** any new `SuttaBridge` function needs a matching stub
  in `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` with the correct
  signature and a dummy return value, for `qmllint` (CLAUDE.md). Any new QML
  component file must be added to the `qml_files` list in `bridges/build.rs`.
- **Threading:** `scan_source` already runs on a spawned thread and reports back
  through the `scan_finished` / `scan_failed` signals
  (`bridges/src/dictionary_manager.rs:451-480`). Materializing a large `.zip`
  should not block the UI thread either; if the copy is slow, it belongs on a
  worker thread with the same signal-based reporting.
- **Keep-screen-awake:** if materialization + scan can take more than a few
  seconds on a large archive, bracket it with
  `AssetManager.set_keep_screen_on(true/false)`, releasing in the completion
  handler on both success and failure (CLAUDE.md).
- **Testing without a Chromebook:** the ChromeOS-specific branches cannot be
  exercised on the developer machine. Structure the code so the
  URL-normalization and scheme-dispatch logic is a **pure, testable Rust
  function** (given a URI string, decide the branch and produce the expected
  encoded form), with only the JNI I/O behind the `cfg`/`#ifdef` gate. Add Rust
  unit tests covering `file://` on Unix and Windows, `content://` with
  `%3A`/`%2F` preserved, an unknown scheme, and a bare path.
- **Regression guard:** add a test asserting that a URI containing `%2F` and
  `%3A` survives the QML→Rust hop unchanged, since that is the defect most
  likely to silently regress.
- **Docs to update:** `docs/android-file-saving-saf.md` should gain a section on
  the **read** path (or a new sibling doc) so the `to_encoded()` rule is stated
  for both directions; `PROJECT_MAP.md` per CLAUDE.md.

## 9. Platform-by-Platform Analysis

How the proposed solution behaves on each supported target, and what to watch
for. **Linux, macOS and Windows are regression surfaces** (they work today); the
two Android targets are where the fix has to earn its keep.

### 9.1 Linux (desktop / AppImage)

- Picker returns `file:///…`. Resolver takes the `file://` branch, no staging, no
  behaviour change. Free-space check (Req. 24) is a no-op cost.
- **Watch:** if Qt routes through `xdg-desktop-portal` (Flatpak-style
  sandboxing, or a portal-enabled desktop), the returned URL points into
  `/run/user/<uid>/doc/<id>/name.zip`. That path is real and readable, so it
  works — but it is a **document-portal proxy that can be revoked**, and it is
  not the path the user thinks they picked. Since the staged copy is only made
  for provider schemes, a large import reads directly from the proxy path over
  the whole extraction. Acceptable, but if flakiness is ever reported here, the
  fix is to stage portal paths too.
- **Watch:** `is_desktop` is `Qt.platform.os !== "android" && !== "ios"`, so all
  four source options (including the three folder ones) are live. The
  `FolderDialog` branch must be migrated too (Req. 6) or it keeps its own copy of
  the bug.

### 9.2 macOS

- Picker returns `file:///…`. Same as Linux.
- **Confirmed non-issue:** `entitlements.plist` enables the **hardened runtime**
  but not `com.apple.security.app-sandbox`, so the app has ordinary filesystem
  access; `com.apple.security.files.user-selected.read-write` is present but only
  binds under App Sandbox. There is no macOS analogue of the Android
  scoped-storage branch. Entitlements are applied only when
  `APPLE_SIGNING_IDENTITY` is set (`build-macos.sh:405-415`), which does not
  change this conclusion.
- **Watch:** filenames on macOS are NFD-normalized Unicode. Pāli diacritics in a
  dictionary filename round-trip through `QUrl` fine, but if the staged filename
  is ever compared for equality against a user-entered label, normalize first.

### 9.3 Windows

- Picker returns `file:///C:/…`. The `file://` branch must keep producing
  `C:/path` — this is the case most easily broken by a careless rewrite, because
  the drive-letter special-case is easy to drop.
- **Real regression risk — UNC paths.** See Req. 7a: the existing
  `qurl_to_local_path` uses `QUrl::path()`, which discards the host, so
  `file://server/share/dict.zip` becomes `/share/dict.zip`. A user importing from
  a network share today already hits this on the *save* path; do not propagate it
  to the read path. Use `toLocalFile()` semantics.
- **Watch:** the portable install mode resolves `SIMSAPA_DIR` relative to the exe
  directory (see [docs/windows-portable-install.md](../docs/windows-portable-install.md)).
  On a USB stick, the extraction target of Req. 23 lands on the stick, which may
  be small and slow. The free-space check (Req. 24) must inspect the **staging
  volume actually used**, not `C:`.

### 9.4 Android phone

- Picker returns `content://com.android.providers.downloads.documents/document/msf%3A1003`
  or similar. Short, shallow encoding — which is exactly why **this platform
  works today despite Defect B**: decoding `%3A`→`:` in a single trailing segment
  often still parses back to the right document. That accidental tolerance is why
  the bug reached production unnoticed.
- After the fix, the same picks go through `to_encoded()` and are handled
  deterministically instead of by luck. **This is the main regression surface for
  the fix** — a phone that works today must still work. Test document import,
  chanting import and Gloss "Open JSON" on a phone, not just dictionary import.
- Memory/disk: per §5.5, a 200 MB import is fine on a modern phone; on a 2–3 GB
  device the chunked copy (Req. 10) is what keeps it so.
- `minSdkVersion 27` … `targetSdkVersion 36`: scoped storage is fully enforced,
  so the SAF-only decision is the only viable one without a manifest change.

### 9.5 Android on Chromebook (the reported platform)

- `Qt.platform.os` is `"android"`, so `is_mobile` is true and only the single-zip
  option is reachable — the reported user had no other route.
- Picks come from ChromeOS volumes (My files, Downloads, Play files, Google
  Drive) surfaced into ARCVM through Chrome's own providers. Two consequences,
  both fixed by this PRD:
  - the document id is **deeply encoded** (`primary%3ADownload%2Ffoo.zip`, or an
    `externalfile%3A…%2F…` payload), so Defect B's decoding is *unrecoverable*
    here, unlike on a phone;
  - the scheme may not be `content://` at all, which Defect A rejects outright.
- **Cannot be tested locally.** This is the single biggest execution risk in the
  PRD. Mitigation is structural: Req. 29 puts the decision logic in a pure
  backend module so the branch selection is unit-testable from a URI string on
  the developer machine, leaving only JNI I/O untestable.
- **Watch — Drive-backed picks are network-backed.** A file "in" Google Drive may
  stream on demand. A 200 MB dictionary staged from Drive over a slow connection
  will take a long time with no progress indication, and the user is likely to
  think the app has hung. The scanning frame currently shows an indeterminate
  progress bar with no text; consider a "Copying file…" label, and apply the
  `AssetManager.set_keep_screen_on` rule (§8) to the staging step, not only the
  scan.
- **Watch — a revoked URI permission.** SAF grants are per-pick and can expire.
  Staging immediately at pick time (as specified) is the right design precisely
  because it does not hold a URI across the user's later import confirmation.

### 9.6 Cross-cutting

- The **one** code path that changes on every platform is Req. 6's migration of
  four call sites. Desktop platforms exercise only the `file://` branch of the new
  resolver, so a desktop test pass proves the migration compiles and the common
  branch is right, but proves nothing about the provider branch.
- `scan_source`'s new URL-scheme rejection (Req. 15) must detect a **scheme**,
  not merely the presence of `:`. A Windows path `C:/Users/…` contains a colon
  and must not be mistaken for a URL; matching on `://` or on a parsed scheme is
  safe, matching on `:` is not.

## 10. Success Metrics

### 10.1 Phase 1 (§4A)

1. The user presses **File Selection Test**, picks the failing `.zip`, presses
   **Run Storage Diagnostics**, and sends back the summary text and `log.txt` —
   with no follow-up questions from us. **This is the phase-1 acceptance test.**
2. The returned `log.txt` states unambiguously whether the picker URL was empty,
   and if not, its scheme and whether its encoded and decoded forms differ —
   i.e. it lands the report in exactly one row of §4A.5.
3. The staging-root comparison of D-12 appears in the log, settling Defect D.2
   as measured fact.
4. No import behaviour changed: the four call sites of §2.6 are untouched in the
   phase-1 diff, and `android/AndroidManifest.xml` is byte-identical.
5. `cd backend && cargo test` and `make qml-test` pass, with unit tests for the
   `backend/src/picker_url.rs` branch selection (D-10) covering `file://` on Unix
   and Windows, `content://` with `%3A`/`%2F` preserved, an unknown scheme, a
   bare path, and an **empty** URL.
6. Qt's `qWarning` output reaches `log.txt` (D-14), verifiable on the developer
   machine by triggering any existing `qWarning` path.

### 10.2 Phase 2 (§5 onwards)

1. The reporting Chromebook user can complete a StarDict `.zip` import
   end-to-end. **This is the primary acceptance test.**
2. Zero remaining occurrences of `Qt.platform.os === "android" && … startsWith("content://")`
   in `assets/qml/` — verifiable with a single `grep`.
3. Zero remaining occurrences of `String(url)` / `.toString()` applied to
   `selectedFile` or `selectedFolder` in the four files of §2.6. (Scope the check
   to those identifiers — `.toString()` has many legitimate uses elsewhere in
   QML, so a bare `grep` for it proves nothing.)
4. No user-visible failure message in the import flow is generic; every failure
   branch names its stage. Verified by code review of each `return`/error path.
5. Existing desktop import behaviour is unchanged: `make qml-test` and
   `cd backend && cargo test` pass, including
   `backend/tests/test_dictionary_import_dir.rs`.
6. `android/AndroidManifest.xml` is byte-identical to its pre-change state.
7. A subsequent Chromebook bug report, if any, can be diagnosed from the user's
   screenshot alone, with no `logcat` request.

## 11. Open Questions

Resolved since the first draft (kept for the record):

- **Q1 — which failure branch hit the user?** ~~*Answerable without a new
  build*~~ — **answered 2026-08-06, and the answer was "none of them."** The log
  shows an **empty** path (§2.1). The claim that the shipped error message
  "already embeds the offending path" was true but useless: the path it embedded
  was the empty string. This is the correction that produced §2.1a and §4A.
- **Q2 — does ChromeOS still emit `externalfile:`?** **Still open, and no longer
  answerable from the shipped build.** The URL never reaches Rust, so nothing
  prints it. Phase 1's D-8(d) is now the only way to find out.
- **Q3 — should messages offer a workaround?** *Yes.* Req. 14a, scoped to the two
  failure reasons where it helps.
- **Q4 — is a size guard needed?** No RAM guard; disk is the constraint. §5.5.
- **Q5 — progress feedback during staging?** *Yes, in scope.* Now §5.3a
  (Reqs. 17b–17d).
- **Q6 — where does the dispatch logic live?** `backend/`. Req. 29.

Still open:

0. **Why is `selectedFile` empty?** (§2.1a.) The central question, and the one
   phase 1 exists to answer. Now has a **source-level mechanism** —
   `QUrl(uri.toString())` at `qandroidplatformfiledialoghelper.cpp:48` producing
   an empty `QUrl` while `accept()` is emitted anyway — but it is not yet proven
   on the device. D-3's unfiltered `FileDialog` discriminates the `nameFilters`
   candidate; the direct-intent capture (tasks 3.10-3.14) discriminates the
   `QUrl`-conversion one.
0a. **Should the diagnostic launch its own `ACTION_OPEN_DOCUMENT`?** Raised
   2026-08-06 by the review. It is the **only** way to see the raw URI, because
   Qt destroys it before app code runs (§2.1a). It uses Qt private API
   (`QAndroidActivityResultReceiver`, `QtAndroidPrivate::startActivity` from
   `QtCore/private/qandroidextras_p.h` — both `Q_CORE_EXPORT` and present in the
   6.9.3 Android kit), which is what Qt's own helper uses but which may shift
   under the pending Qt upgrade. Decision recorded at task 3.10.

   **ANSWERED 2026-08-07: no.** Phase 1 ships **public API only**, so the
   diagnostic stays valid across the planned Qt upgrade rather than becoming
   something that must be re-verified against a new kit before its own output
   can be trusted. The cost is accepted and is narrow: an empty URL is confirmed
   but its raw string cannot be recovered, which — given §2.1a's source-level
   mechanism — likely points at a Qt-level fix anyway. D-3's unfiltered dialog
   and the D-12 staging facts are unaffected. If the raw URI does become
   necessary, the **preferred** route is no longer the private API but a small
   Java class in `android/` launching `ACTION_OPEN_DOCUMENT` and returning
   `intent.getData().toString()` over JNI: public API throughout, immune to the
   upgrade, and it also removes this question's coupling to Qt's release
   schedule.

   **REVERSED 2026-08-07: yes, use the private API.** The answer above was
   sound reasoning resting on an assumption that had not been checked. It was
   then checked against qtbase's **6.11** branch — the release this project is
   upgrading to — and it does not hold. Three measured facts:

   1. **The private API is unchanged in 6.11.** `qandroidextras_p.h` on the
      6.11 branch still declares `class Q_CORE_EXPORT
      QAndroidActivityResultReceiver` with the same pure-virtual
      `handleActivityResult(int, int, const QJniObject &)`, and all three
      `QtAndroidPrivate::startActivity` overloads with identical signatures —
      byte-compatible with the 6.9.3 kit. The file has had four commits since
      2022 and the most recent (2026-03-03, "Add default security headers") is
      cosmetic. The upgrade this question was deferring to is a **non-event**
      for this API.
   2. **The blast radius is smaller than it reads.** `Qt6::CorePrivate` is an
      *interface* target — verified in the local kit, `Qt6CoreConfig.cmake:133`
      propagates only `INTERFACE_INCLUDE_DIRECTORIES`. It adds header paths,
      **not a library**: no new `.so` in the AAB, no ABI-slice growth, no
      manifest change. None of the packaging hazards of
      [android-multi-abi-and-chromeos.md](../docs/android-multi-abi-and-chromeos.md)
      are in play. And the failure mode is a **compile error at upgrade time**,
      not silent misbehaviour, on ~80 lines of `#ifdef`-gated, deletable
      diagnostic code.
   3. **6.11 does not fix the bug, so the upgrade is not a substitute for
      measuring.** `qandroidplatformfiledialoghelper.cpp` on the 6.11 branch
      still does `m_selectedFile.append(QUrl(uri.toString()))`, still emits
      `accept()`, and still contains **zero** `qWarning` calls. Its
      `uri.isValid()` test guards the *Java* `Uri`, not the resulting `QUrl`, so
      §2.1a's hole is open in 6.11 exactly as in 6.9.3. Unlike the Thai/Gboard
      Shift bug of
      [android-soft-keyboard.md](../docs/android-soft-keyboard.md) §4, this one
      is **not** waiting for us upstream.

   The custom-Java alternative is genuinely upgrade-proof but is now the more
   expensive and riskier option for a throwaway diagnostic: it costs a new Java
   class **plus an `<activity>` entry in `android/AndroidManifest.xml`**, which
   breaks phase-1 success metric 4's byte-identical-manifest check and touches
   the one file with a history of filtering this app off Chromebooks.

   **Terms of the reversal**, so the risk stays where it was measured:

   - the private include is confined to the **diagnostic**; phase 2's import
     path must not acquire a dependency on it, where a future build break would
     take a shipping feature down with it;
   - the raw string feeds the **same** `PickerUrlFacts` pipeline (task 3.13),
     never a second report shape;
   - the module comment records that the private include is deliberate, scoped,
     and expected to be **deleted** once the report comes back.
1. **`delete_temp_import_folder` signature change (Req. 19)** is a breaking change
   to an existing bridge function. Its only callers are
   `DocumentImportDialog.qml:360` and `:376` (verified 2026-07-31) — re-confirm
   at implementation time.
2. **Should staging apply to desktop portal paths too?** (§9.1.) Currently no. If
   AppImage-on-portal imports are ever reported as flaky, this is the lever.
3. **Should Req. 25** (single-extraction probe) be split into its own task? It is
   the largest optional win here and is independent of the URL handling.

## Appendix A — Diagnostic requests for the reporting user

> **SUPERSEDED 2026-08-06. Do not send this.** Its premise — that the current
> release is self-diagnosing because the error message embeds the offending path
> — was tested against the user's actual log and failed: the path is **empty**
> (§2.1). A.1's table has no row for that, and A.2's differential test would
> return the identical empty message from every location, distinguishing nothing.
>
> **Use Appendix B instead.** A.3 (environment details) and A.4 (tone) are still
> good and are carried forward there. The rest is kept only as a record of the
> reasoning that had to be corrected.

**No new build is required for any of this.** Two facts make the current release
self-diagnosing, which was not obvious when this PRD was drafted:

- the error text already contains the offending path —
  `format!("Path not found: {}", path.display())` — so its *shape* identifies the
  branch directly;
- `bridges/src/dictionary_manager.rs:474` logs `scan_source failed: …` through the
  Rust logger, which writes to the app log file, and **About → log file list →
  "Copy Contents"** puts that log on the clipboard, ready to paste into an email.

The one gap: the native `qWarning`s in `copy_content_uri_to_temp_file` do **not**
reach that log (there is no `qInstallMessageHandler` — Req. 17a fixes this). So
if staging itself failed, the log will show the QML-side outcome but not the
native reason.

### A.1 The single highest-value ask

> Could you send a screenshot of the whole error message, including the text
> after "Path not found:"? Or, in the app: **Help → About**, scroll to the log
> files, press **Copy Contents** on the most recent one, and paste it into a
> reply.

Read the path shape to identify the branch:

| What follows "Path not found:" | Branch | Conclusion |
|---|---|---|
| `content://…` | Defect B | The URI was corrupted by decoding, or reached Rust unconverted. |
| `externalfile://…` or another scheme | Defect A.1 | ChromeOS does still emit non-`content://` URIs — **answers Q2 directly**. |
| `/storage/emulated/0/…` or another absolute path | Defect A.2 | Sandboxed `file://` path; scoped storage is the blocker. |
| A path with `:` or `/` where `%3A` / `%2F` should be | Defect B | Confirms the pretty-decoding corruption. |

### A.2 Differential test — which locations work

Ask the user to retry the same `.zip` from several places, reporting the exact
message each time:

1. **Downloads** (Files app → My files → Downloads);
2. **Play files** (the Android-side storage, if the Chromebook shows it);
3. **Google Drive**, if that is where the file came from;
4. after **copying the file into Downloads** and picking it there — this is the
   workaround of Req. 14a, so the answer tells us whether it is worth
   recommending.

Interpretation: if Play files succeeds while Drive or My files fails, the
provider scheme is the discriminator (Defect A.1). If every location fails with
an identical message, it is more likely the QML gate or the decoding. If (4)
succeeds, the workaround text is validated and can ship in the release notes as
an interim answer.

### A.3 Environment details

Cheap to ask, and they pin down the ARC generation and the picker implementation:

1. Chromebook model, and **Settings → About ChromeOS** version;
2. Simsapa version, and whether it was installed from **Google Play** or
   sideloaded (the beta `.apk`) — the About dialog's **"Copy App Info"** button
   provides version and paths in one paste;
3. which app appeared when the file picker opened — the ChromeOS **Files** window
   or a plainer Android document picker;
4. roughly how large the dictionary `.zip` is (relevant to §5.5, and to whether
   the copy is slow rather than failing).

### A.4 Tone

The user is reporting a bug in good faith and is not an Android developer. Lead
with the screenshot request (A.1) alone — it is very likely sufficient on its
own, and A.2/A.3 are follow-ups if it is not. Avoid asking for `adb` or logcat;
that is precisely the gap Req. 17a exists to close.

---

## Appendix B — What to ask the user, once the phase-1 build is ready

**Added 2026-08-06, superseding Appendix A.** This build carries **two**
buttons, and both reports come back in one round trip: the storage diagnostics
summary (the SD-card fulltext bug) and `log.txt` (this bug). The user is the
same person for both.

### B.1 The ordering that matters

The **File Selection Test must be run before the log is copied**, because it is
what writes the interesting lines *into* `log.txt`. A user who copies the log
first and tests afterwards sends an empty result and we lose the round trip. The
message below is written so that the steps are impossible to reorder.

### B.2 The message

> I have a test build with two new buttons that should tell me what is going
> wrong, without you needing to describe anything. Could you install it and do
> these four steps in order?
>
> 1. Open **Help → About** and press **File Selection Test**. When the file
>    chooser opens, pick **the same dictionary .zip file that would not import**.
>    The app will not import it — it only looks at what the file chooser handed
>    over.
> 2. Press **Run Storage Diagnostics** on the same screen, wait for it to
>    finish, then press **Copy** and paste the text into your reply.
> 3. Still in the About window, scroll down to the list of log files, press
>    **Copy Contents** on the most recent one, and paste that into your reply as
>    well. (If it is too long to paste, use **Save As…** and attach the file.)
> 4. If it is not too much trouble: repeat step 1 two or three more times,
>    choosing the same file from different places — your **Downloads** folder,
>    **Play files**, and **Google Drive** if that is where it came from. Then
>    copy the log again. Each attempt adds a few lines and it helps a lot to see
>    which locations behave differently.

Step 4 is Appendix A.2's differential test, now producing machine-readable lines
rather than a screenshot of an identical error message.

### B.3 Environment details

Still worth having, still cheap, and mostly answered automatically now — the
storage diagnostics summary already carries the app version, platform and
Android API level, and D-12 adds the staging paths. What it does **not** carry,
and is still worth one sentence in the reply:

1. Chromebook model, and **Settings → About ChromeOS** version;
2. whether Simsapa was installed from **Google Play** or sideloaded;
3. which app appeared when the file chooser opened — the ChromeOS **Files**
   window or a plainer Android document picker. **This one is now the most
   valuable of the three**: §2.1a's leading hypothesis is that Qt cannot map
   what the ARC picker returned, so which picker appeared is a direct clue.

   *Note (2026-08-07):* since D-3a the Android test launches the app's **own**
   `ACTION_OPEN_DOCUMENT` rather than going through Qt's `FileDialog`, so what
   appears is whatever ChromeOS resolves that intent to. If the user reports a
   *different* chooser here than the one they saw when the import failed, that
   difference is itself a finding — it would mean Qt's dialog and a plain
   `ACTION_OPEN_DOCUMENT` reach different pickers, and the two blocks are not
   measuring the same thing.

### B.4 Tone

Unchanged from A.4, and it is the reason the flow is built this way. The user is
reporting a bug in good faith and is not an Android developer. Ask for button
presses and pastes, never for `adb` or logcat — closing that gap is exactly what
D-14 is for.

### B.5 Reading what comes back

- `log.txt` → grep for `FILE-SELECTION-TEST:` (D-7). Each run is one block with a
  counter and timestamp (D-11). Take the block to §4A.5's decision-gate table.
  **On an Android block, read the raw-intent rows first**: the raw URI and
  `QUrl(raw)` validity (D-8h) sit upstream of the scheme and encoding lines, and
  they can settle the whole question on their own. Only if the raw URI parses
  cleanly do the original rows apply.
- The storage summary → the storage PRD's own decision gate (§9 there).
- If step 4 produced several blocks, compare their scheme and encoded-form lines
  across locations before concluding anything from any single one.
