# File Selection Test

**Feature PRD:** `tasks/2026-07-31-180502-prd---picker-url-handling-and-chromebook-import-failure.md` (§4A)
**Task list:** `tasks/2026-07-31-180502-tasks-picker-url-handling-and-chromebook-import-failure.md`
**Sibling diagnostic:** [storage-diagnostics.md](./storage-diagnostics.md) — shipped in the same build, to the same user.

## 1. Why this exists

A Chromebook user could not import a StarDict `.zip`. The app said:

```
Scan failed: Path not found:
```

**Nothing follows the colon.** The path was the empty string — verified with
`cat -A` in `feedback-and-bug-reports/log-chromebook.txt:127-128`, across two
attempts two minutes apart. So the picker returned no URL at all, and none of
the four URL-handling defects the PRD had catalogued (§2.2-§2.5) is demonstrated
by the report. A build fixing all four could ship and leave that user exactly as
broken.

Worse, the question could not be answered by email. The shipped error prints the
path it was given, and that path is empty: no scheme, no encoding, no filename.
Asking the user to retry from Downloads, Play files and Drive would have
returned the identical empty message from every location.

So this feature ships a button that **changes no import behaviour** and measures
what the picker actually returns on the affected device, writing the whole
report to `log.txt` — the file the user is already able to send.

**None of it is throwaway.** `probe_document_uri` in
`backend/src/android_saf.rs` *is* the PRD's Req. 8 fix for Defect C, in its
final location, and `backend/src/picker_url.rs` *is* Req. 29's classifier. Phase
1 wires them only into the diagnostic; phase 2 flips them into the four import
call sites.

## 2. Where it lives

| Piece | File |
|---|---|
| Classifier, report builder, staging facts | `backend/src/picker_url.rs` |
| Provider-URI probe (JNI) | `backend/src/android_saf.rs` (`probe_document_uri`) |
| Raw `ACTION_OPEN_DOCUMENT` picker | `cpp/android_raw_pick.{h,cpp}` |
| Bridge invokables + signal | `bridges/src/sutta_bridge.rs` |
| Button, picker, result dialog | `assets/qml/AboutDialog.qml` |
| Qt→log message handler | `cpp/gui.cpp` (`simsapa_message_handler`) |

`backend/src/picker_url.rs` is **Qt-free by construction** (PRD Req. 29 / D-10).
Every branch decision is a pure function over owned `String`s, which is what
makes the ChromeOS behaviour — unreproducible on any machine we have —
unit-testable here. The two facts that need Qt are extracted on the bridge side
and passed in: `PickerUrlFacts` (from the `QUrl`) and `qurl_of_raw_is_valid` (a
plain `bool`).

## 3. One button, two pickers

**One press opens exactly one picker, and which one depends on the platform**
(D-3a):

| Platform | Picker | Started by | Result arrives via |
|---|---|---|---|
| desktop | Qt `FileDialog`, no `nameFilters` | `FileDialog.onAccepted` → `run_file_selection_test(url)` | the invokable's own worker |
| Android | the app's own `ACTION_OPEN_DOCUMENT` | `start_file_selection_test_raw_pick()` | the native activity-result callback, `raw_document_pick_result_c()` |

**Why Android does not use Qt's `FileDialog`.** Qt's own Android helper,
`qandroidplatformfiledialoghelper.cpp:48`, does:

```cpp
m_selectedFile.append(QUrl(uri.toString()));   // QString parse, TolerantMode
…
Q_EMIT accept();                                // emitted regardless
```

If the picker's URI does not parse as a valid `QUrl`, the result is an **empty
`QUrl`** and `accept()` fires anyway — which is an exact, sufficient mechanism
for the reported bug. The raw string is destroyed *inside Qt* before any app
code runs, and `currentFile`, `currentFiles` and `selectedFiles` are all fed
from the same `m_selectedFile` list, so on that path they are **all empty
together**. A diagnostic built on Qt's `FileDialog` could only confirm "empty",
which the user's log already said.

Running *both* pickers was rejected: it would put two consecutive choosers in
front of the user, contradicting the instructions they are sent. The accepted
cost is that nothing now exercises Qt's `setType` / `EXTRA_MIME_TYPES` mapping —
which only becomes interesting if the raw URI turns out to parse cleanly (the
last row of §5.2 below).

**Neither picker sets a file-type filter** (D-3, D-3b). The real import dialog's
`nameFilters: ["StarDict archives (*.zip)"]` is itself one of the suspects, and
a diagnostic must not inherit the configuration it is testing.

The raw picker uses request code **`51305`**, deliberately not Qt's `1305`
(`qandroidplatformfiledialoghelper.cpp:24`) — activity results are dispatched by
request code, and a clash would cross the two dialogs' results over.

### 3.1 The private-Qt include

`cpp/android_raw_pick.cpp` is the **only** file in the tree that includes
private Qt API (`QtCore/private/qandroidextras_p.h`, for
`QtAndroidPrivate::startActivity`) — the same API Qt's own file dialog uses. The
decision, and its reversal, is recorded in PRD §11 Q0a; the short form:

- the header is **unchanged on the 6.11 branch** (same signatures, last commit
  cosmetic), so the pending Qt upgrade is a non-event for it;
- `Qt6::CorePrivate` is an **interface** target — include paths, not a library —
  so there is no new `.so`, no ABI-slice growth, no manifest change;
- 6.11 does **not** fix the bug (the helper still carries the line above and
  still emits zero `qWarning`s), so waiting for the upgrade is not a substitute
  for measuring.

**Keep it confined to the diagnostic.** Phase 2's import path must not acquire a
dependency on it, and this file is expected to be **deleted** once the report
comes back. `CMakeLists.txt` links `Qt6::CorePrivate` on Android only, and
deliberately not via `${qt_modules}` — that list is also handed to
`cxx_qt_import_crate(QT_MODULES)`, which resolves names through qmake.

## 4. Reading a `FILE-SELECTION-TEST:` block

Every line carries the `FILE-SELECTION-TEST:` prefix so the block survives a
truncated paste and is greppable on its own. Each run is bracketed by
`===== run N begin =====` / `===== run N end =====`; the counter is
process-global, so repeated presses (from Downloads, Play files, Drive) produce
distinguishable blocks within one app session.

**Read the block top to bottom — that is the causal order.** The raw-pick lines
sit upstream of every `QUrl` question, so they come first.

### Header

| Line | Meaning |
|---|---|
| `timestamp`, `platform`, `android_api_level` | when and where |
| `picker` | **which picker produced this block** — `raw ACTION_OPEN_DOCUMENT intent` or `Qt FileDialog`. Blocks from different pickers are not the same measurement and must never be compared as though they were. |

### Raw pick (Android only, D-8h)

| Line | Meaning |
|---|---|
| `raw_branch` | which branch of the activity result produced the string: `intent-getData`, `intent-getClipData`, `cancelled`, `no-uri`, `no-intent`, `unsupported-platform` |
| `raw_uri_length`, `raw_uri` | **the URI exactly as the picker returned it**, never round-tripped through a `QUrl`. This is the deliverable of the whole round trip. |
| `qurl_of_raw_is_valid` | **the single most valuable line in the report.** It reproduces `qandroidplatformfiledialoghelper.cpp:48` using the same constructor Qt uses (cxx-qt-lib's `QUrl::from(&QString)` resolves through `qurl_init_from_qstring` to `QUrl(QString)` in `TolerantMode`). `no` for a non-empty `raw_uri` means Qt's conversion is where the URL is lost. |

`no-intent` is what the two Android early-failure paths deliver (a null intent,
or a JNI exception while building it). They originally returned without
delivering anything, which left the button disabled forever — **only a delivered
result completes a run**, and that is the same rule the `cancelled` branch obeys.

### The `QUrl` view (D-8a-e)

| Line | Meaning |
|---|---|
| `url_empty_or_invalid` | **measured first.** `YES` is the finding, not a reason to stop — the block continues to the staging facts regardless. |
| `url_encoded` | `to_encoded()`, the fully-encoded form — the same call `save_file` makes |
| `url_decoded` | `toString()`, the pretty-decoded form |
| `encoding_differs` | **this one boolean is the Defect B measurement.** A difference proves the corruption; identity rules it out for that pick. See §6 before "fixing" anything here. |
| `url_scheme`, `url_host`, `url_path_segments` | answers "does ChromeOS still emit `externalfile:`?" directly. Segments are counted on the **encoded** form, so a `%2F` inside a segment is not miscounted as a separator. |
| `branch` | `Empty` / `LocalFile` / `Provider { scheme }` / `BarePath` |

Then one section per branch:

- **`LocalFile`** — `local_file` (from `toLocalFile()` semantics, **never**
  `QUrl::path()`, which drops the host and silently breaks a Windows UNC pick)
  and `local_file_exists`.
- **`Provider`** — `provider_*`, below. Any non-empty scheme takes this branch;
  schemes are deliberately **not** allowlisted, since rejecting an unfamiliar
  provider is the defect that hid this failure.
- **`BarePath`** — should not come from a picker. Saying so is how we would
  learn that it did.

### The provider probe (D-8f/g)

| Line | Meaning |
|---|---|
| `provider_opened` | did `ContentResolver.openInputStream(Uri.parse(uri))` return a usable stream |
| `provider_display_name`, `provider_size` | `OpenableColumns`, in one cursor query. A provider is entitled to supply neither; `(none)` is not a fault. |
| `provider_bytes_read`, `provider_reached_cap` | the read is **capped at 4 MB** and the bytes are discarded. Reaching the cap is a **success**, not a truncation error. |
| `provider_open_ms`, `provider_read_ms` | the only place the "a Drive-backed pick streams over the network" concern is ever actually measured |
| `provider_error` | names **which step** failed — URI parse, resolver open, query, read |

The probe goes through `ContentResolver`, **not** `QFile(content_uri)` (D-9):
`QFile` works only for `content://` via `QAndroidContentFileEngine`, so it would
fail on precisely the non-`content://` scheme the test exists to detect.

### `raw_provider_*` — the second probe, and when it runs

A raw URI that `QUrl` rejects must still be read directly. `Uri.parse` and
`openInputStream` take a plain string and never needed a `QUrl` at all, so when
the `QUrl` route did **not** already read the document, the raw URI is probed on
its own under a `raw_provider_` prefix. A raw URI that opens and reads while
`QUrl` rejects it turns "bypass the conversion" from a hypothesis into a
demonstrated phase-2 fix.

It is deliberately **not** read twice — a provider read can stream over a
network. When the URL round-tripped unchanged and was already read, the line is:

```
raw_provider: (not re-read: the URL above round-tripped through QUrl unchanged and has already been read)
```

### Staging facts (D-12)

Appended to **every** block whatever the URL branch, because they are
independent of the pick — a user who only ever produces empty-URL blocks still
supplies them.

| Line | Meaning |
|---|---|
| `staging_cpp_root` | `QStandardPaths::TempLocation` + `/simsapa-imports`, the root the C++ writer uses |
| `staging_rust_root` | `std::env::temp_dir()` + `/simsapa-imports`, the root the Rust cleanup is pointed at |
| `staging_roots_differ` | **settles Defect D.2 as measured fact.** See §6. |
| `staging_cpp_*` (and `staging_rust_*` when the roots differ) | exists / file count / total bytes / age of oldest entry — the evidence for or against the unbounded-footprint claim. Absence is a normal reported fact, not an error. |
| `staging_space_*` | free/total on the staging volume via `fs4::statvfs`, measured at the nearest existing ancestor when the root does not exist yet |

## 5. The decision gate

Take the returned block to PRD §4A.5 and read off the row. **On an Android block
the raw-intent table is read first** — it sits upstream of everything else.

### 5.1 Raw-intent rows (read first)

| `raw_uri` | `qurl_of_raw_is_valid` | Conclusion |
|---|---|---|
| non-empty | **no** | The bug is Qt's `QUrl(QString)` conversion at `qandroidplatformfiledialoghelper.cpp:48`. **None of PRD §5's requirements is the fix**, and since 6.11 carries the line unchanged, an upstream fix cannot be waited for. The work becomes: normalize or bypass that conversion. Compare the raw string against `QUrl`'s parsing rules to find *what* it rejects. |
| non-empty | **yes** | Picker and conversion are both fine; the loss is downstream of Qt's dialog. The leading suspect is the `nameFilters` → `setType`/`EXTRA_MIME_TYPES` mapping that D-3a deliberately does not exercise — this is the case that earns a second round trip with a Qt-`FileDialog` run. Continue to §5.2. |
| empty, `raw_branch: cancelled` | — | The user backed out. Not a finding; ask for another run. |
| empty, `raw_branch: no-uri` / `no-intent` | — | The picker reported success but returned no URI — a case Qt's helper drops silently. The failure is upstream of every URL question in the PRD. |

### 5.2 `QUrl` rows

| `url_empty_or_invalid` | `encoding_differs` | scheme | Conclusion |
|---|---|---|---|
| **YES** | — | — | **Defect E confirmed as the user's bug.** The URL→path rewrite is *not* the fix; investigate the Qt Android `FileDialog` → ARC picker mapping. |
| no | **yes** | any | **Defect B confirmed.** `QUrl` + `to_encoded()` is the fix, as drafted. |
| no | no | not `content://` | **Defect A.1 confirmed.** The provider reader is the fix. |
| no | no | `file://`, `local_file_exists: no` | **Defect A.2 confirmed.** Scoped storage; an honest message plus the Downloads workaround is all that is available. |
| no, provider read **succeeds** | no | `content://` | The pick is fine; the failure is downstream. Re-triage from the staging facts. |

## 6. Four measured states that are normal

Do not "fix" the report when it says any of these.

1. **`encoding_differs: no`.** Qt's `toString()` defaults to `PrettyDecoded`,
   which does **not** decode `%2F` or `%3A` inside a path — they are delimiters.
   Defect B may therefore be milder than PRD §2.3 asserts. Confirmed on device
   (Android 16, `content://com.android.externalstorage.documents/…`): both forms
   came back **identical**, with `%3A` and `%2F` preserved in each. That is
   data, not a defect in the diagnostic.
2. **`staging_roots_differ: no`.** PRD §2.5 asserts that
   `QStandardPaths::TempLocation` and `std::env::temp_dir()` differ on Android,
   making the cleanup "very likely a silent no-op". **On an Android 16 device
   both resolve to `/data/user/0/<pkg>/cache/simsapa-imports`** — identical. If
   this holds generally, Req. 20 is a non-issue and Req. 21a loses its D.2 half.
   Measured on one device and one Android version; not yet general.
3. **`staging_cpp_exists: false`.** The test stages nothing (D-4), so on a device
   that has never completed an import the folder legitimately does not exist.
   This doubles as the on-device proof that the diagnostic writes nothing.
4. **`provider_reached_cap: true`.** The read stopped at 4 MB by design. A
   success, not a truncation.

## 7. What it must never do

- **Import nothing, stage nothing, modify no app data** (D-4). Every
  write-shaped call in `picker_url.rs` lives inside its `mod tests`; the probe
  opens an input stream and discards the bytes. There is nothing to `Drop`-guard
  because nothing is created — do not add a cleanup guard for a file that does
  not exist.
- **No file contents in the log.** The URL and paths are acceptable; contents
  are not. Guarded by a test that writes a file containing an API-key-shaped
  string and asserts the block does not carry it.
- **No QML-side string handling of the URL.** `selectedFile` goes straight into
  `run_file_selection_test(url)` — no `String(...)`, no `strip_file_scheme`, no
  inspection. Any of those would re-introduce the corruption being measured.
- **No polling.** The Android result arrives through a listener
  (`set_raw_pick_listener`), which reaches the bridge singleton through a
  registered `CxxQtThread<SuttaBridge>` rather than a global object pointer. The
  callback is the event.

## 8. Qt's own warnings now reach `log.txt` (D-14)

Independently useful, and it improves the *existing* release's diagnosability:
`cpp/gui.cpp` installs a `qInstallMessageHandler` right after
`init_app_globals()` and before the `QApplication` — the same slot the
render-loop and palette pre-reads occupy, late enough that the logger's data
directory is resolvable and early enough to catch the QML engine's warnings.

- `QtWarningMsg` / `QtCriticalMsg` / `QtFatalMsg` → `log_error_c`;
  `QtInfoMsg` → `log_info_c`; **`QtDebugMsg` is dropped entirely** — Qt's debug
  stream would bury the `FILE-SELECTION-TEST:` block in the file we are asking
  the user to paste.
- Lines are prefixed `Qt: ` and carry `context.category` when set, so a
  Qt-internal warning is distinguishable from the app's own.
- It **chains** to the previous handler, so `adb logcat` and Qt Creator's
  Application Output are unaffected and the change is purely additive.

Note that this does **not** catch the bug it was motivated by:
`qandroidplatformfiledialoghelper.cpp` contains **zero** `qWarning`/`qCWarning`
calls and loses the URL silently. It was worth doing anyway, and the five real
failure messages in `copy_content_uri_to_temp_file` plus the two in `copy_file`
were converted to `log_error_c` at the same time.

**Deleted while there:** `list_qrc_assets()` and
`copy_qrc_app_assets_to_internal_storage()` in `cpp/utils.cpp`, verified
callable from nowhere in the tree. They carried 11 of the file's 24 `qWarning`s,
several of them *debug tracing* misusing `qWarning` (one line per resource
file) — converting those would have pushed per-file spam into the very log the
user emails.

## 9. Related

- [storage-diagnostics.md](./storage-diagnostics.md) — the sibling phase-1
  diagnostic, shipped in the same build to the same user.
- [android-file-saving-saf.md](./android-file-saving-saf.md) — the **write**
  path. The `to_encoded()` rule stated there applies identically to reading, and
  this feature is its read-side mirror.
- [android-multi-abi-and-chromeos.md](./android-multi-abi-and-chromeos.md) — why
  `android/AndroidManifest.xml` is byte-identical in this change, and why it
  must stay that way.
