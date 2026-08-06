# Tasks — "File Selection Test" (phase 1 of the picker-URL PRD)

PRD: `tasks/2026-07-31-180502-prd---picker-url-handling-and-chromebook-import-failure.md`
Scope: **§4A only** (requirements D-1…D-14). Phase 2 (§5, Reqs. 1–30) is
deliberately **not** planned here — the phase-1 report decides what phase 2
should be (PRD §4A.5).

Sibling phase-1 work already shipped: `tasks/2026-08-05-201545-tasks-run-storage-diagnostics.md`.
This build ships alongside it and goes to the **same user** (PRD header).

## Component analysis (what has to exist, and what blocks what)

| # | Component | Kind | Depends on | PRD reqs |
|---|---|---|---|---|
| C1 | Native failure messages reach `log.txt` (conversion + message handler) | C++ | — | D-14 |
| C2 | `backend/src/picker_url.rs` — Qt-free URL classifier | Rust (pure, testable) | — | D-10, D-8a–e |
| C3 | Document-URI probe in `android_saf.rs` (resolver-attach split + capped read) | Rust/JNI | C2 | D-8f/g, D-9 |
| C4 | Import-staging facts (C++ vs Rust temp roots, folder census, free space) | Rust + 1 C++ accessor | — | D-12 |
| C5 | Report builder + INFO logging, prefix, run counter | Rust | C2–C4 | D-7, D-11 |
| C6 | `SuttaBridge` invokable taking `&QUrl` + completion signal + qmllint stub | Bridge | C5 | D-5, D-6 |
| C7 | QML: About button, unfiltered `FileDialog`, on-screen outcome | QML | C6 | D-1, D-2, D-3, D-6, D-13 |
| C8 | Unit tests | Rust tests | C2–C5 | metric 5 |
| C9 | Docs + the Appendix B email | docs | C1–C7 | metrics 1–3 |

D-4 (import nothing, modify no app data) and the §4A.4 non-goals are
**verification constraints** rather than components; task 8.3 checks them against
the diff.

Every phase-1 requirement D-1…D-14 maps to at least one component above.

## Notes on the existing codebase (assessed before planning)

- **`backend/src/android_saf.rs` already holds the whole JNI stack in Rust** —
  `jni 0.21`, `ndk_context`, and `attach()` (`:47`) which yields
  `(AttachGuard, ContentResolver, Uri, tree_doc_id)`. It was written for the
  **write** path (`write_to_tree_uri`, `child_exists`). This is why D-9 is
  implemented **in Rust here, not in `cpp/utils.cpp`** as PRD Req. 8 words it —
  see the note on task 3.0. `attach()` is **tree-URI specific** (it calls
  `DocumentsContract.getTreeDocumentId`), so it needs splitting before a plain
  document URI can reuse it.
- **`cxx_qt_lib::QUrl` already exposes everything D-8 needs**: `is_valid()`,
  `scheme_or_default()`, `to_encoded()` (→ `QByteArray`), `to_qstring()` (the
  *decoded* `toString()` form), `to_local_file()`, `host_or_default()`, `path()`.
  No new Qt plumbing is required. **Use exactly these Rust names** — an earlier
  draft of this line used the C++ spellings; see review finding 8.
- **`save_file(folder_url: &QUrl, …)`** (`bridges/src/sutta_bridge.rs:3388`, with
  the `to_encoded()` recovery at `:614` and `:3484`) is the exact call-shape
  precedent for passing a QML `url` straight into Rust. PRD §8 calls this work
  "the read-side mirror" of it.
- **`run_storage_diagnostics`** (`bridges/src/sutta_bridge.rs:3964`) is the
  invokable pattern to copy verbatim: `qt_thread()` → `thread::spawn` →
  `catch_unwind` → queue the signal. Its signal is declared at `:822-824` with
  the `#[qsignal] #[cxx_name = …]` convention, and its `qmllint` stub sits in the
  "Search index signals" group at
  `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml:47`.
- **C++ functions are reached from Rust** by declaring them in the
  `unsafe extern "C++"` block — `get_android_package_name()` /
  `get_installer_package_name()` at `sutta_bridge.rs:708-709`, defined in
  `cpp/utils.h:14-15`. This is the mechanism task 4.1 uses for the
  `QStandardPaths::TempLocation` accessor.
- **The Rust logger is lazy and failure-tolerant.** `log_info_c` / `log_error_c`
  (`backend/src/logger.rs:685`, `:718`) null-check and UTF-8-check, then call
  `with_logger()` (`:547`), which `get_or_init`s and, on `Logger::new()` failure,
  installs a **disabled logger that silently does nothing** (`:559-568`). No
  panic path, thread-safe. `cpp/utils.cpp` already calls it at `:323`, `:333`,
  `:339`, `:423`, `:514` — from storage enumeration that runs *before*
  `init_app_globals()` — so early calls are already proven in production. The
  established idiom is `log_info_c(QString(...).arg(x).toUtf8().constData())`
  (`:339`); the temporary `QByteArray` lives to the end of the full expression,
  so this is safe as an argument.
- **`AboutDialog.qml`** already has `Logger { id: logger }` (`:14`), the
  invisible-`TextEdit` `clipboard_helper` (`:71`), a `FolderDialog` for log saving
  (`:295`), and a **full-width `ColumnLayout`** button column at `:255-291`
  holding "Copy App Info" (`:261`), "Run Storage Diagnostics" (`:273`) and
  "Close" (`:286`). The column replaced a `RowLayout` because three buttons on
  one row overflowed a phone screen — a **fourth** button makes keeping the
  column mandatory.
- **No new QML file is created**, so `bridges/build.rs` needs **no** change. D-6
  explicitly does not want a results window; `log.txt` is the deliverable.
- `fs4 = "0.13"`, `memmap2 = "0.9"` and unix-only `libc` are **already** direct
  dependencies of `backend` (`backend/Cargo.toml:53-73`), added by the storage
  diagnostics work. Task 4.3's free-space call needs no new dependency.
- `backend/src/storage_diagnostics.rs` is the model for a `String`-returning,
  section-headed, unit-testable report builder.

## Findings from the pre-planning investigation (verified, not assumed)

1. **Converting `qWarning` → `log_error_c` is not dangerous.** The logger cannot
   crash an early caller (see above), the externs are already declared in
   `cpp/utils.cpp:24-25`, and the same file already does it. CLAUDE.md's note —
   *"do not add new ones, and prefer converting them when touching that code for
   another reason"* — is **permission, not prohibition**, and phase 1 is working
   in this area.
2. **But a blanket conversion of all 24 is wrong.**
   `list_qrc_assets()` (`cpp/utils.cpp:856`) and
   `copy_qrc_app_assets_to_internal_storage()` (`:870`) are **never called
   anywhere** — verified across the whole tree; only their definitions and the
   declarations at `cpp/utils.h:19-20` exist. They carry **11 of the 24**
   `qWarning`s, and several of those are *debug tracing* misusing `qWarning`
   (`<< "list_qrc_assets()"`, one line per resource file, `<< length()`).
   Converting them would push per-file spam into the very file the user emails.
3. **The 7 that matter** are the five in `copy_content_uri_to_temp_file`
   (`:648`, `:657`, `:664`, `:672`, `:683`) and the two in `copy_file` (`:565`,
   `:573`) — genuine failures directly on the import path.
4. **The message handler is still worth having, but — CORRECTED by review
   finding 7 — it will *not* catch this bug.** Qt's own warnings (the QML
   engine's especially) cannot be converted because we do not own them, and
   routing them into `log.txt` is a real gain. But this draft went on to claim the
   handler was the most likely place the answer was hiding, on the assumption that
   Qt warns when it fails to map the picker result. **It does not:**
   `qandroidplatformfiledialoghelper.cpp` contains **zero** `qWarning`/`qCWarning`
   calls and loses the URL silently. Read finding 6 and 7 before prioritising
   task 1.0.
5. **`AboutDialog` must own the keep-screen-on bracket here.** The storage
   diagnostics task list explicitly said *not* to give `AboutDialog` an
   `AssetManager`, because the results window owned that run (its FR-10c). There
   is **no results window** in this feature, so `AboutDialog` is the sole owner
   and does need one. Do not read the older instruction as forbidding it.

## Review findings (2026-08-06) — read before starting

A verification pass over Qt 6.9.3's own source and the app's code, after the task
list was drafted. **Finding 6 is the important one: it identifies a precise
source-level mechanism for the reported bug, and it invalidates part of the
diagnostic's design as originally specified.** Findings 7 and 8 correct claims
made above.

### 6. Qt destroys the raw picker URI before QML can see it — and still emits `accepted`

`qtbase/src/plugins/platforms/android/qandroidplatformfiledialoghelper.cpp:48`:

```cpp
const QJniObject uri = intent.callObjectMethod("getData", "()Landroid/net/Uri;");
if (uri.isValid()) {
    takePersistableUriPermission(uri);
    m_selectedFile.append(QUrl(uri.toString()));   // <-- QString parse, TolerantMode
    Q_EMIT fileSelected(m_selectedFile.constFirst());
    Q_EMIT currentChanged(m_selectedFile.constFirst());
    Q_EMIT accept();
    return true;
}
```

Qt takes the Java `Uri.toString()` and hands it to the **`QUrl(QString)`
constructor**. If the ARC picker's URI does not parse as a valid `QUrl`, the
result is an **empty/invalid `QUrl`** — and `accept()` is emitted anyway
(`:51`). QML's `onAccepted` therefore fires with an empty `selectedFile`.

**That is an exact, sufficient mechanism for the observed log** (PRD §2.1: two
attempts, both `Path not found:` with an empty path). It raises one of PRD
§2.1a's three candidates from speculation to strongly-supported, and it has
three consequences the task list must absorb:

- **D-8(a)'s fallbacks are worthless.** `currentFile`, `currentFiles` and
  `selectedFiles` are all fed from the same `m_selectedFile` list, so on this
  path they are *all* empty. The raw URI string is destroyed inside Qt before any
  app code runs. Task 7.2a can log them, but must not be expected to recover
  anything.
- **The diagnostic as specified cannot answer the question in the empty case.**
  It would confirm "the URL is empty" — which the existing log already told us —
  and nothing more. That is a wasted round trip with the user, which is the exact
  failure phase 1 exists to prevent.
- The `nameFilters` candidate is *not* eliminated: the helper still sets
  `setType` and `EXTRA_MIME_TYPES` (`:162-167`), so an unsatisfiable MIME filter
  could change what the picker returns. Keep D-3's unfiltered dialog.

**Remedy — capture the URI before Qt converts it** (new sub-tasks 3.10–3.14).
`QAndroidActivityResultReceiver` and `QtAndroidPrivate::startActivity` are
exported from `QtCore/private/qandroidextras_p.h` in the Android kit (verified in
the 6.9.3 `android_arm64_v8a` install), which is what Qt's own helper uses. The
diagnostic can launch its own `ACTION_OPEN_DOCUMENT` and read
`intent.getData().toString()` as a **raw Java string**, never passing it through
`QUrl`. See task 3.10 for the trade-off and the decision.

### 7. The message handler will **not** catch this failure — correcting finding 4

`grep -c 'qWarning\|qCWarning' qandroidplatformfiledialoghelper.cpp` → **0**.
The whole file emits no diagnostics whatsoever; the empty-`QUrl` conversion at
`:48` is silent. Finding 4 above claimed this was "the most likely place the
answer is hiding" — **that was wrong**. Task 1.0 remains worth doing (our own 7
converted messages, QML engine warnings, and every other Qt warning genuinely do
reach `log.txt` through it), but it is **not** the mechanism that answers this
bug, and its priority relative to 3.10 drops accordingly.

### 8. Concrete corrections to the tasks below

- **`Pin<&mut Self>` + `&QUrl` has no precedent in this codebase.** Every `&QUrl`
  method is `self: &SuttaBridge` (`sutta_bridge.rs:1263`, `:1284`), and every
  spawn-and-signal invokable takes no `&QUrl`. It *should* compile —
  `Pin<&mut SuttaBridge>` with `&QString` reference parameters is common
  (`results_page`, `:897`), and `type QUrl` is already registered in the extern
  block at `:699` — but **verify it with a throwaway compile before task 6.2 is
  built on**, because the whole bridge design assumes it.
- **cxx-qt-lib `QUrl` Rust method names** (from
  `cxx-qt-lib/src/core/qurl.rs`): `is_valid()` (not `isValid()`);
  `to_local_file() -> Option<QString>` which returns `None` unless
  `is_local_file()`, with `to_local_file_or_default()` as the raw form;
  `scheme_or_default() -> QString` and `scheme() -> Option<QString>`;
  `to_qstring()` for the `toString()` form; `host_or_default()`;
  `to_encoded() -> QByteArray`. Use these names.
- **`current_platform()` (`storage_diagnostics.rs:1875`) and `android_api_level()`
  (`:1892`) are private.** Task 5.2 says to reuse them — that requires making
  them `pub` (a one-word change, no behaviour effect) rather than writing a
  second copy.
- **Free/total space:** `storage_diagnostics.rs:282` uses `fs4::statvfs(path)`,
  which yields both figures in one call. Task 4.4 names
  `fs4::available_space`; prefer `statvfs` for consistency with the existing
  code.
- **PRD D-8(a) names the dialog's `folder`** — that is the Qt 5 property name.
  Qt 6.9.3's `QQuickFileDialog` has **`currentFolder`** (verified in
  `qquickfiledialog_p.h:35`), along with `currentFile`, `currentFiles` and
  `selectedFiles`. The task list already says `currentFolder`; the PRD line is
  wrong and is corrected there.
- **PRD §2.3's premise is itself unverified.** Qt's `toString()` defaults to
  `PrettyDecoded`, which does **not** decode `%2F` inside a path (it is a
  delimiter). "Defect B" may therefore be milder than §2.3 asserts. This is a
  strength of the design, not a problem — task 2.3's `encoding_differs` measures
  it either way — but **nobody should "fix" the report if encoded and decoded
  come back identical on a phone.** That is data, not a defect in the diagnostic.
- **`AboutDialog.qml:31-34`** carries a comment stating that this dialog does
  *not* own the storage-diagnostics run and only calls `open_and_run()`. Adding
  an `AssetManager` in task 7.3 will read as a contradiction unless that comment
  is extended to say the *file selection test* is owned here because it has no
  results window (finding 5).
- **No new imports are needed in `AboutDialog.qml`**: `QtQuick.Dialogs` is
  already imported (`:7`) for the existing `FolderDialog` (`:295`), the root is
  an `ApplicationWindow` with `required property int extra_top_margin` (`:29`),
  and it is already instantiated and bound in `SuttaSearchWindow.qml`. Tasks 7.x
  add no new QML file and no new binding.

## Relevant Files

- `cpp/utils.cpp` — convert the 7 real failure `qWarning`s (finding 3); delete
  the two dead qrc functions (finding 2); add the `QStandardPaths::TempLocation`
  staging-root accessor for D-12.
- `cpp/utils.h` — remove the two dead declarations (`:19-20`); declare the new
  accessor.
- `cpp/gui.cpp` — install the `qInstallMessageHandler` (D-14). Insertion point is
  after `init_app_globals()` (`:403`) and before `QApplication` (`:493`), the
  same slot the render-loop and palette pre-reads already use.
- `backend/src/picker_url.rs` — **new.** The Qt-free classifier, the staging-facts
  collector, the report builder and the `run_file_selection_test()` entry point
  (D-10, D-7, D-8, D-11, D-12), with its unit tests in a `mod tests` at the
  bottom (the convention `storage_diagnostics.rs` follows).
- `backend/src/lib.rs` — declare the new module.
- `backend/src/android_saf.rs` — split `attach()` (`:47`) into a shared
  resolver-attach plus the tree-specific part; add the document-URI probe
  (D-8f/g, D-9). This is phase-2 code in its final location.
- `bridges/src/sutta_bridge.rs` — the `#[qsignal] fileSelectionTestCompleted`
  (beside `storageDiagnosticsCompleted` at `:822`), the
  `run_file_selection_test(url: &QUrl)` invokable (modelled on `:3964`), and the
  `unsafe extern "C++"` declaration of the new C++ accessor (beside `:708-709`).
- `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` — `qmllint` stubs for the
  new method and signal; the signal goes in the same group as
  `storageDiagnosticsCompleted` (`:47`).
- `assets/qml/AboutDialog.qml` — the "File Selection Test" button in the button
  column (`:255-291`), an unfiltered `FileDialog`, an `AssetManager`, the
  `Connections` handler and the on-screen outcome line.
- `docs/file-selection-test.md` — **new.** What the test measures, how to read a
  `FILE-SELECTION-TEST:` block, and the §4A.5 decision-gate table.
- `docs/android-file-saving-saf.md` — gains a **read**-path cross-reference, so
  the `to_encoded()` rule is stated for both directions (PRD §8).
- `CLAUDE.md` / `PROJECT_MAP.md` — pointers to the new doc and module.
- **Not touched:** `assets/qml/DictionaryImportDialog.qml`,
  `DocumentImportDialog.qml`, `ChantingPracticeWindow.qml`, `GlossTab.qml`
  (the four call sites of PRD §2.6), `android/AndroidManifest.xml`, and
  `bridges/build.rs` (no new QML file).

### Notes

- Rust tests: `cd backend && cargo test`; a single test with `cargo test <name>`.
  QML: `make qml-test`. Full build: `make build -B`.
- Do **not** run the GUI to test (CLAUDE.md); compile-verify and unit-test. The
  on-device check is task 8.4, performed by the user.
- Every file-existence check uses `try_exists()`, never `.exists()` (PRD Req. 28).
- QML logging uses `Logger { id: logger }` with a **single concatenated string**
  (D-7); never the `console` API.
- New C++ logs use `log_info_c()` / `log_error_c()`, never `qInfo()`/`qWarning()`.
- All new JNI code is `#[cfg(target_os = "android")]` / `#ifdef Q_OS_ANDROID`
  gated (PRD Req. 27).

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, check it off in this markdown file by
changing `- [ ]` to `- [x]`. Update the file after completing each sub-task, not
just after completing an entire parent task.

## Tasks

---

### 1.0 Make the native half of the import path visible in `log.txt` (D-14)

**Specs to keep in mind.** This is the one task that improves the *existing*
release's diagnosability, independently of everything else — the reporting user
cannot run `adb`, and today every native failure in the import path is
logcat-only. Two mechanisms, addressing two different sources: **conversion**
for messages we author, **a message handler** for Qt's own. Do not treat them as
alternatives (findings 1–4). Nothing here changes control flow.

**Depends on:** nothing. **Blocks:** nothing — do it first because it is
self-contained and de-risks every later on-device observation.

- [ ] 1.1 Convert the **five** `qWarning`s in `copy_content_uri_to_temp_file`
      (`cpp/utils.cpp:648`, `:657`, `:664`, `:672`, `:683`) to `log_error_c`,
      using the established idiom at `:339`:
      `log_error_c(QString("…%1…").arg(x).toUtf8().constData())`. Preserve each
      message's text and every interpolated value (the `QFile::errorString()`
      calls especially — they are the native reason we are missing). Change **no**
      control flow, no return values, no early exits.
- [ ] 1.2 Convert the **two** `qWarning`s in `copy_file` (`cpp/utils.cpp:565`,
      `:573`) the same way. These already build a `ret_msg` `QString`, so the
      conversion is `log_error_c(ret_msg.toUtf8().constData())`.
- [ ] 1.3 Delete the two **dead** functions `list_qrc_assets()`
      (`cpp/utils.cpp:856`) and `copy_qrc_app_assets_to_internal_storage()`
      (`:870`), and their declarations at `cpp/utils.h:19-20`. Verified callable
      from nowhere in the tree (finding 2). Re-run the grep before deleting, in
      case the tree has moved on. If anything does call them, **stop and leave
      them alone** — do not convert their 11 tracing `qWarning`s, which would
      spam the log file the user emails.
- [ ] 1.4 Leave `cpp/global_hotkey_x11.cpp`'s three `qWarning`s alone. They are
      X11-only, off the import path, and outside this feature's reason to touch
      the file.
- [ ] 1.5 Install a `qInstallMessageHandler` in `cpp/gui.cpp`, after
      `init_app_globals()` (`:403`) and before `QApplication` (`:493`) — the
      slot the render-loop and palette pre-reads already occupy, and late enough
      that the logger's data dir is resolvable. Map `QtWarningMsg` →
      `log_error_c`, `QtCriticalMsg` / `QtFatalMsg` → `log_error_c` with the
      severity in the text, and `QtInfoMsg` → `log_info_c`.
- [ ] 1.5a **Drop `QtDebugMsg` entirely.** Qt's debug stream is high-volume and
      would bury the `FILE-SELECTION-TEST:` block in the file we are asking the
      user to paste.
- [ ] 1.5b Prefix every handler-routed line distinctly (e.g. `Qt: `) so a
      maintainer reading `log.txt` can tell a Qt-internal warning from one of the
      app's own messages. Include `context.category` when it is set — QML engine
      warnings carry `qml`, which is exactly the category PRD §2.1a's hypothesis
      would surface under.
- [ ] 1.5c Keep the handler body **free of any Qt call that could itself warn**,
      and do not re-enter Qt logging from inside it. Compose the string with
      `QString`/`QByteArray` only and hand it to `log_*_c`. A handler that warns
      while handling a warning recurses until the stack is gone.
- [ ] 1.5d Chain to the previous handler returned by `qInstallMessageHandler`, or
      deliberately do not, and **write which and why in a comment**. Not chaining
      means logcat loses Qt's warnings on Android (they now go to `log.txt`
      instead); chaining means they appear twice on desktop stderr. Recommended:
      chain, so `adb logcat` and Qt Creator's Application Output are unaffected
      and this task is purely additive.
- [ ] 1.6 Build (`make build -B`) and verify on the developer machine that a Qt
      warning reaches `log.txt` — triggering any existing `qWarning` path is
      enough (metric 6). Confirm the app still starts and the log is not flooded
      (1.5a).

---

### 2.0 `backend/src/picker_url.rs` — the Qt-free URL classifier

**Specs to keep in mind.** D-10 and PRD Req. 29: the branch decision must be a
**pure Rust function over strings**, so the ChromeOS behaviour we cannot
reproduce locally is still unit-testable here. The Qt side extracts the facts;
this module decides. Critically, **D-8(a) is measured first** — an empty URL is
the only failure the bug report actually demonstrates (PRD §2.1), and the module
must model it as a first-class branch rather than as "some other scheme".

**Depends on:** nothing. **Blocks:** 3.0, 4.0, 5.0.

- [ ] 2.1 Create `backend/src/picker_url.rs`, declare it in `backend/src/lib.rs`,
      and define `PickerUrlFacts` — the Qt-extracted inputs, all owned `String`s
      plus one `bool`: `is_valid`, `encoded` (`to_encoded()`), `decoded`
      (`to_qstring()`), `scheme`, `host`, `local_file` (`toLocalFile()`).
      Keeping the struct Qt-free is what makes the tests possible.
- [ ] 2.2 Define `PickerBranch` — `Empty`, `LocalFile`, `Provider { scheme }`,
      `BarePath` — and `classify(&PickerUrlFacts) -> PickerBranch`, mirroring
      PRD Req. 4(a)–(d) plus the new empty case. `Empty` when `!is_valid` **or**
      `encoded` is empty; `LocalFile` for `file`; `BarePath` for an empty scheme
      on a non-empty string; `Provider` for every other non-empty scheme
      (`content`, `externalfile`, anything else) — Req. 4(c) deliberately does
      not allowlist schemes.
- [ ] 2.2a Do **not** treat a Windows drive letter as a scheme. `C:/Users/…`
      parses with scheme `c` in some URL libraries; the classifier works from
      Qt's already-parsed `scheme` field, so this is a **test** to write rather
      than logic to add (PRD §9.6 makes the same point for Req. 15).
- [ ] 2.3 Add `encoding_differs(&PickerUrlFacts) -> bool` comparing `encoded`
      against `decoded`. This single boolean **is** the Defect B measurement
      (D-8c): a difference proves the corruption, identity rules it out for that
      pick. Keep it a named function so the report and the tests share one
      definition.
- [ ] 2.4 Add a process-global run counter (`AtomicU64`) and
      `next_run_number() -> u64` (D-11), so repeated presses produce
      distinguishable blocks.
- [ ] 2.5 Unit-test `classify` and `encoding_differs` against: `file:///path`
      (Unix), `file:///C:/path` (Windows), `file://server/share/x.zip` (UNC — the
      host must survive, Req. 7a), `content://…/document/primary%3ADownload%2Ffoo.zip`
      (the `%3A`/`%2F` **preserved** in `encoded` and **decoded** in `decoded`,
      so `encoding_differs` is true), `externalfile://…`, a bare
      `/home/user/x.zip`, a bare `C:/Users/x.zip`, and — the case that matters —
      an **empty** URL both as `is_valid: false` and as an empty `encoded`
      (metric 5).

---

### 3.0 The document-URI probe in `android_saf.rs` (D-8f/g, D-9)

**Specs to keep in mind.** PRD Req. 8 / D-9 word this as a fix to
`copy_content_uri_to_temp_file` in `cpp/utils.cpp`. **It is implemented in Rust
instead**, in `backend/src/android_saf.rs`, because that file already carries the
whole JNI stack for the write path — same `jni 0.21` pin, same `ndk_context`,
same error-string discipline — and PRD §8 itself frames this work as "the
read-side mirror" of `save_file`. This is a placement decision, not a scope
change; record it in the module doc comment so phase 2 does not re-litigate it.
The existing `attach()` (`:47`) is **tree-URI specific** — it calls
`DocumentsContract.getTreeDocumentId`, which a plain document URI has no answer
for — so it must be split before it can be reused.

**Depends on:** 2.0 (the `Provider` branch selects this path).
**Blocks:** 5.0.

- [ ] 3.1 Split `attach()` (`backend/src/android_saf.rs:47`) into
      `attach_resolver(vm) -> (AttachGuard, ContentResolver)` and a thin
      `attach_tree(vm, tree_uri)` that calls it and then does `Uri.parse` +
      `getTreeDocumentId`. Keep `write_to_tree_uri` and `child_exists` behaviour
      **byte-identical** — this is a refactor, and their tests/behaviour are the
      regression surface.
- [ ] 3.2 Add `probe_document_uri(uri: &str, cap_bytes: usize) -> DocumentProbe`
      taking the **fully-encoded** URI string (never a pretty-decoded one — the
      `to_encoded()` trap of `docs/android-file-saving-saf.md` applies identically
      to reading). It parses with `Uri.parse` and opens via
      **`ContentResolver.openInputStream`** — *not* `QFile` (D-9), which only
      works for `content://` through `QAndroidContentFileEngine` and would fail on
      exactly the non-`content://` scheme this probe exists to detect.
- [ ] 3.3 Have `DocumentProbe` carry every field D-8(f)/(g) needs, each
      independently `Option`al so a partial failure still reports what it learned:
      `opened: bool`, `display_name`, `size`, `bytes_read`, `open_ms`,
      `read_ms`, and `error: Option<String>` naming **which** step failed (URI
      parse, resolver open, query, read).
- [ ] 3.4 Resolve the display name via `OpenableColumns.DISPLAY_NAME` and the size
      via `OpenableColumns.SIZE`, in one cursor query, tolerating a null cursor
      and a missing column without failing the whole probe (PRD Req. 9's
      fallbacks; the sanitisation half of Req. 9 belongs to phase 2, which
      actually writes a file).
- [ ] 3.5 **Cap the read** at `cap_bytes` (a few MB — 4 MB is ample) and read in
      fixed-size chunks, discarding the bytes. D-4 forbids staging anything, and
      the test must never pull a 200 MB archive across a Drive connection. Report
      `bytes_read` and stop cleanly at the cap; reaching the cap is a **success**,
      not a truncation error.
- [ ] 3.6 Close the stream on **every** path, including the error paths, and
      write no file anywhere (D-4). There is nothing to `Drop`-guard because
      nothing is created — state that in a comment so a later reader does not add
      a cleanup guard for a file that does not exist.
- [ ] 3.7 Time the open and the capped read separately (D-8g). §9.5's
      Drive-streaming concern is a latency question and this is the only place it
      is ever measured.
- [ ] 3.8 Gate the whole probe with `#[cfg(target_os = "android")]` and provide a
      non-Android stub returning a `DocumentProbe` whose error says the platform
      has no provider-backed reader (PRD Req. 27), so the desktop build compiles
      and the desktop report reads honestly.
- [ ] 3.9 Confirm `cd backend && cargo test` still passes and the write path is
      untouched in behaviour — 3.1 is the only edit to shipping code in this task.

**Raw-URI capture (review finding 6).** Without these, an empty-URL result tells
us only what the existing log already told us, and the round trip with the user
is wasted. Qt's helper destroys the raw string at
`qandroidplatformfiledialoghelper.cpp:48` before any app code runs, so the only
way to see it is to run our own picker intent.

- [ ] 3.10 **DECISION — confirm before implementing 3.11–3.14.** This uses Qt
      **private** API (`QtCore/private/qandroidextras_p.h`:
      `QAndroidActivityResultReceiver` at `:96`, `QtAndroidPrivate::startActivity`
      at `:199-205`, both `Q_CORE_EXPORT`, verified present in the 6.9.3
      `android_arm64_v8a` kit). It is exactly what Qt's own file-dialog helper
      uses, so it is well-trodden, but private API can change and this project has
      a Qt upgrade pending
      (`docs/android-qt-upgrade-considerations.md`). The alternative is to ship
      without it and accept that an empty result confirms only "Qt handed QML
      nothing" — which, combined with the source reading in finding 6, may already
      be enough to move to a Qt-level fix. Record the decision here either way.
- [ ] 3.11 Add an Android-only "raw pick" path: build an `ACTION_OPEN_DOCUMENT`
      intent with `CATEGORY_OPENABLE` and `setType("*/*")` (no MIME filter — the
      D-3 rationale applies here too), and launch it with
      `QtAndroidPrivate::startActivity` using a **request code that cannot collide
      with Qt's own `1305`** (`qandroidplatformfiledialoghelper.cpp:24`).
- [ ] 3.12 In the result receiver, read `intent.getData()` and record
      **`uri.toString()` as a raw Java string**, before any `QUrl` exists. Also
      record `QUrl(uri.toString()).isValid()` — the single most valuable line in
      the whole report, because it reproduces Qt's `:48` conversion and shows
      directly whether that is where the URL is lost. Handle the `getClipData()`
      branch (`:57-69`) too, and the "neither" case, which Qt leaves silently
      emitting nothing.
- [ ] 3.13 Feed the raw string into the **same** `PickerUrlFacts` pipeline (task
      2.1) so both paths produce the same report shape, with one line naming which
      path produced the block (Qt `FileDialog` vs raw intent). Do **not** fork the
      report builder.
- [ ] 3.14 Gate all of it behind `#[cfg(target_os = "android")]` / `#ifdef
      Q_OS_ANDROID` with a desktop stub, and confirm the desktop build neither
      links nor references the private header (PRD Req. 27).

---

### 4.0 Import-staging facts (D-12)

**Specs to keep in mind.** This section needs **no user interaction at all** — it
is pure measurement that settles Defect D (PRD §2.5), which the PRD currently
calls "very likely a silent no-op". The comparison is the point: the C++ writer
uses `QStandardPaths::TempLocation` (`cpp/utils.cpp:645`) and the Rust cleanup
uses `std::env::temp_dir()` (`bridges/src/sutta_bridge.rs:3687`). Print both and
say plainly whether they differ.

**Depends on:** 2.1 (the module). **Blocks:** 5.0.

- [ ] 4.1 Add a C++ accessor returning the staging root
      (`QStandardPaths::writableLocation(QStandardPaths::TempLocation) + "/simsapa-imports"`)
      to `cpp/utils.cpp` + `cpp/utils.h`, and declare it in the
      `unsafe extern "C++"` block of `bridges/src/sutta_bridge.rs` beside
      `get_android_package_name()` (`:708-709`). **Derive it from the same
      expression `copy_content_uri_to_temp_file` uses** — ideally by extracting
      that expression into the new function and calling it from both, so the two
      cannot drift.
- [ ] 4.2 In `picker_url.rs`, add `collect_staging_facts(cpp_root: &str) -> StagingFacts`
      recording: the C++ root (passed in — the backend stays Qt-free), the Rust
      root (`std::env::temp_dir().join("simsapa-imports")`), and an explicit
      `roots_differ: bool`. The bridge supplies `cpp_root`; do not try to reach
      Qt from the backend.
- [ ] 4.3 Census the staging folder, tolerating its absence as a normal reported
      fact rather than an error: exists (`try_exists()`), file count, total size,
      and the age of the oldest entry — the evidence for or against PRD Req. 21a's
      unbounded-footprint claim.
- [ ] 4.4 Report free/total space on the staging volume via **`fs4::statvfs`**, matching
      `storage_diagnostics.rs:282` (finding 8) — already a direct dependency
      (`backend/Cargo.toml:53`), cross-platform, no new code per platform. This is the input PRD Req. 24's threshold will need.
- [ ] 4.5 Census **both** roots when they differ, not just the C++ one. The whole
      point is to show which directory actually holds the staged files and which
      one the cleanup is pointed at; reporting only one cannot demonstrate the
      mismatch.
- [ ] 4.6 Unit-test `collect_staging_facts` against a temp directory with known
      contents, and against a non-existent root (must report cleanly, not error).

---

### 5.0 The report builder and the `run_file_selection_test()` entry point

**Specs to keep in mind.** The deliverable of this whole feature is a block of
INFO lines in `log.txt` (D-7) — there is no results window (§4A.4). Every line
carries the `FILE-SELECTION-TEST:` prefix so it is greppable and survives a
truncated paste, and every block carries a run number and timestamp (D-11) so
repeated picks from Downloads / Play files / Drive are distinguishable. **D-8(a)
first, and never stop at the first blank**: if the URL is empty, say so and keep
reporting whatever else is knowable, because "empty" is the finding.

**Depends on:** 2.0, 3.0, 4.0. **Blocks:** 6.0.

- [ ] 5.1 Implement
      `run_file_selection_test(facts: &PickerUrlFacts, cpp_staging_root: &str) -> String`
      in `picker_url.rs`, returning the whole block as a `String` (the pattern
      `storage_diagnostics::run_storage_diagnostics()` follows, and what makes it
      unit-testable).
- [ ] 5.2 Emit the header: run number (2.4), timestamp, and the platform — plus
      Android API level where applicable, reusing `storage_diagnostics.rs`'s `current_platform()` (`:1875`) and
      `android_api_level()` (`:1892`) rather than writing a second copy — **both are
      private today and must be made `pub`** (finding 8).
- [ ] 5.3 Emit the URL lines in D-8's order, one labelled line each: **(a)
      empty/invalid first**, then encoded, decoded, an explicit
      `encoding_differs: yes/no` line (2.3), scheme, host, path segment count.
      Where the URL is empty, emit the empty verdict and then continue to the
      staging facts — the block must never end early (D-8a).
- [ ] 5.4 For the `LocalFile` branch, emit the `toLocalFile()` path and its
      `try_exists()` result (D-8e, Req. 7a). Do **not** use `QUrl::path()`
      anywhere in this feature; it drops the host and silently breaks Windows UNC
      picks.
- [ ] 5.5 For the `Provider` branch, call `probe_document_uri` (3.2) with the
      **encoded** URI and emit its fields, including the failing step on error
      (D-8f, D-8g).
- [ ] 5.6 For `BarePath`, emit the path and its `try_exists()` — it should not
      occur from a picker, and saying so is how we would learn that it did.
- [ ] 5.7 Append the staging facts (4.2–4.5) to every block, whatever the URL
      branch. They are independent of the pick, and a user who only ever produces
      empty-URL blocks still supplies them.
- [ ] 5.8 Log the whole block through the Rust logger at **INFO** (D-7), in one
      call or in clearly contiguous lines that reassemble, and also return it so
      the bridge can put a one-line outcome on screen.
- [ ] 5.9 Add a short `outcome_line(&PickerUrlFacts, …) -> String` producing the
      **plain-language** one-liner for D-6/D-13 — "The file picker did not return
      a file." for the empty case. It must not print `Path not found:`, and it is
      the wording model for phase 2's Req. 13, so keep it to one sentence a
      non-developer can act on.
- [ ] 5.10 Assert by test that the block contains no file **contents** and no
      `api_key`-shaped text (PRD Req. 17: the URL and paths are acceptable, file
      contents are not) — 3.5 discards the bytes it reads, and this test guards
      that.
- [ ] 5.11 Unit-test the builder end-to-end against fixture `PickerUrlFacts` for:
      empty URL, `file://` that exists, `file://` that does not, a `content://`
      with differing encoded/decoded forms, and an unknown scheme. Assert the
      prefix is on every line and the run number increments across calls.

---

### 6.0 Bridge wiring: a `&QUrl`-taking invokable with a completion signal

**Specs to keep in mind.** CXX-Qt invokables run on the **calling (QML) thread**,
so the run must be spawned (D-5); `run_storage_diagnostics`
(`bridges/src/sutta_bridge.rs:3964`) is the pattern to copy, with its signal at
`:822-824`. The `QUrl` must be passed **as a `QUrl`**, never as a string from
QML — that is Defect B's whole lesson (PRD Req. 2), and `save_file(folder_url:
&QUrl, …)` (`:3388`) is the proven precedent. Any new `SuttaBridge` method
**and** signal needs a `qmllint` stub (PRD §8).

**Depends on:** 5.0. **Blocks:** 7.0.

- [ ] 6.1 Add `#[qsignal] #[cxx_name = "fileSelectionTestCompleted"]
      fn file_selection_test_completed(self: Pin<&mut SuttaBridge>, success: bool, outcome: QString)`
      beside `storage_diagnostics_completed` (`:822-824`), following the same
      convention. `outcome` carries the D-6 one-liner, not the whole block — the
      block goes to the log.
- [ ] 6.1a **Verify `Pin<&mut Self>` + `&QUrl` compiles before building on it**
      (finding 8). There is no precedent in this codebase: every `&QUrl` method is
      `self: &SuttaBridge`, and every spawn-and-signal invokable takes no `&QUrl`.
      A one-line throwaway invokable settles it in one `make build -B`. If it does
      **not** compile, the fallback is a `&self` invokable that extracts the facts
      and hands them to a separate `Pin<&mut Self>` method — do **not** fall back
      to passing the URL as a `QString` from QML, which reintroduces the exact
      corruption being measured.
- [ ] 6.2 Add `#[qinvokable] run_file_selection_test(self: Pin<&mut SuttaBridge>, url: &QUrl)`.
      Extract the `PickerUrlFacts` from the `QUrl` **on the calling thread**, using
      the cxx-qt-lib Rust names (finding 8): `is_valid()`, `to_encoded()`,
      `to_qstring()`, `scheme_or_default()`, `host_or_default()`,
      `to_local_file()` (which is `None` unless `is_local_file()`) — into owned
      `String`s, then move those
      into the worker. `QUrl` is not `Send`; extracting first is what makes the
      spawn legal, and it is also the moment the encoding is preserved.
- [ ] 6.2a Recover the encoded form exactly as `save_bytes_to_folder` does at
      `:614`: `String::from_utf8_lossy(url.to_encoded().as_slice()).to_string()`.
      Do **not** use `to_qstring()`, `to_display_string()` or `path()` for it —
      those are the pretty-decoded forms and are only captured separately, as the
      *measurement* of D-8(c).
- [ ] 6.3 Fetch the C++ staging root (4.1) on the calling thread too, and pass the
      resulting `String` into the worker — the same reason as 6.2.
- [ ] 6.4 Spawn the worker with `catch_unwind` (copying `:3966-3988`), emit
      `success: false` with the panic message rather than losing the signal, and
      queue the completion back through `qt_thread()`.
- [ ] 6.5 Add the `qmllint` stubs to
      `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`: the signal beside
      `storageDiagnosticsCompleted` (`:47`) and a trivial
      `run_file_selection_test(url: url)` function stub beside the other bridge
      methods.
- [ ] 6.6 `make build -B` and confirm the generated QML type exposes both the
      method and the signal.

---

### 7.0 QML: the "File Selection Test" button and the picker flow

**Specs to keep in mind.** `AboutDialog.qml`'s bottom button area is a
**full-width `ColumnLayout`** (`:255-291`) precisely because three buttons on a
row overflowed a phone screen and clipped "Close" — a **fourth** button makes
that non-negotiable. The button goes **between** "Copy App Info" (`:261`) and
"Run Storage Diagnostics" (`:273`) so the visual order matches the order
Appendix B asks the user to press them (D-1). The `FileDialog` takes **no
`nameFilters`** (D-3) — the existing `.zip` filter is a suspect in PRD §2.1a and
a diagnostic must not inherit the configuration it is testing. **`AboutDialog`
owns the run here** (finding 5): unlike the storage diagnostics, there is no
results window, so the `AssetManager` bracket and the `Connections` live in this
file.

**Depends on:** 6.0. **Blocks:** 8.0.

- [ ] 7.1 Add the **"File Selection Test"** button to the `ColumnLayout` at
      `AboutDialog.qml:255-291`, between "Copy App Info" and "Run Storage
      Diagnostics", with `Layout.fillWidth: true` like its siblings. **No
      platform gate** — D-2 wants it on desktop too, where it exercises the
      `file://` branch a maintainer can actually read.
- [ ] 7.2 Add a `FileDialog` with **no `nameFilters`** (D-3) and a title naming
      the purpose. On `onAccepted`, pass `selectedFile` **straight** into
      `SuttaBridge.run_file_selection_test(selectedFile)` — no `String(...)`, no
      `strip_file_scheme`, no JavaScript inspection of the URL whatsoever
      (PRD Req. 2). Any QML-side string handling would re-introduce the very
      corruption being measured.
- [ ] 7.2a Log the QML-side view of the pick before the call, through
      `Logger { id: logger }` (already at `:14`) with a **single concatenated
      string** (D-7): the run being started, and — because this is the case under
      investigation — whether `selectedFile` is empty as QML sees it, plus
      `selectedFiles.length` and the dialog's `currentFolder`. If Qt handed QML
      nothing, this is the line that proves it independently of the Rust side.
      **Expect all of them to be empty together** — they are fed from one
      `m_selectedFile` list inside Qt (finding 6), so they corroborate rather than
      recover. The raw URI comes from tasks 3.11-3.13, not from here.
- [ ] 7.2b Handle `onRejected` by logging a cancelled test, so a user who backs
      out of the picker does not leave a maintainer wondering whether the button
      worked.
- [ ] 7.3 Give `AboutDialog` its own `AssetManager { id: manager }` and bracket
      the run with `set_keep_screen_on(true)` before the invokable and `(false)`
      in the completion handler on **both** success and failure (D-5). Finding 5:
      the storage-diagnostics task list's instruction *not* to add an
      `AssetManager` here applied to a run owned by its results window and does
      not apply to this feature. **Extend the comment at `AboutDialog.qml:31-34`**
      to say so, or the new `AssetManager` reads as a contradiction of it
      (finding 8).
- [ ] 7.4 Add a `Connections` on `SuttaBridge` handling
      `onFileSelectionTestCompleted` — set the on-screen outcome text, re-enable
      the button, release the keep-screen-on lock. Guard with an "initiated here"
      boolean: the signal is process-global, and although `AboutDialog` is
      currently the only listener, the guard is what keeps that safe when
      Appendix B's step 4 has the user run it repeatedly.
- [ ] 7.5 Show the outcome **on screen** (D-6): a single wrapping `Label` under
      the button, carrying the 5.9 one-liner plus a fixed reminder that the detail
      is in the log file listed above. Disable the button and show a busy state
      while the run is in flight (D-5).
- [ ] 7.6 Ensure the empty-URL case reads in **plain words** — "The file picker
      did not return a file." — and never surfaces `Path not found:` (D-13).
- [ ] 7.7 Confirm **no new QML file** was created, so `bridges/build.rs` needs no
      change. If a component is factored out later, it must be added to
      `qml_files` (CLAUDE.md) — but D-6 does not want one.
- [ ] 7.8 Run `make qml-test`; confirm `qmllint` is clean and no `console.*` call
      was introduced (D-7).

---

### 8.0 Tests, non-goal verification, docs, and the build to send

**Specs to keep in mind.** Phase-1 metrics 4–6 are the ones an implementer can
get wrong without noticing: **no import behaviour may change**, the manifest must
be byte-identical, and the four call sites of PRD §2.6 must be untouched. The
feature is only useful if the user's log comes back readable, so the doc and the
email are part of the deliverable, not an afterthought.

**Depends on:** 1.0–7.0.

- [ ] 8.1 Consolidate the unit tests from 2.5, 4.6, 5.10 and 5.11 and confirm
      `cd backend && cargo test` and `make qml-test` pass (metric 5). Record any
      pre-existing timing-assertion drift separately rather than as a regression.
- [ ] 8.2 Verify the classifier covers every §4A.5 decision-gate row, so each
      possible report lands in exactly one of them (metric 2). Add a test per row
      if any is unreachable from the fixtures already written.
- [ ] 8.3 Diff the branch against the §4A.4 non-goals and confirm, explicitly:
      `DictionaryImportDialog.qml`, `DocumentImportDialog.qml`,
      `ChantingPracticeWindow.qml` and `GlossTab.qml` are **untouched**;
      `strip_file_scheme` and `file_url_to_path` still exist unchanged;
      `android/AndroidManifest.xml` is **byte-identical**; no network call was
      added; nothing is staged or written into the import folders (D-4); and
      `bridges/build.rs` is unchanged (metric 4).
- [ ] 8.4 **On-device check by the user, not the agent** (CLAUDE.md): build
      `make android-beta-debug`, install, and confirm on a phone that the button
      appears in the right position, the picker opens with **no** file-type
      filter, a normal `content://` pick produces a complete block in `log.txt`,
      and the four buttons all fit without clipping — the defect the storage
      diagnostics work hit at its task 8.7a, now with a fourth button.
- [ ] 8.5 Write `docs/file-selection-test.md`: what each D-8 line means, how to
      read a `FILE-SELECTION-TEST:` block, the §4A.5 decision-gate table, the
      note that the `android_saf.rs` probe is phase-2 code wired only into the
      diagnostic (task 3.0), and the finding-2 record that the two qrc functions
      were dead when deleted.
- [ ] 8.6 Add the **read**-path cross-reference to
      `docs/android-file-saving-saf.md` so the `to_encoded()` rule is stated for
      both directions (PRD §8), and update `CLAUDE.md`'s notable-docs list and
      `PROJECT_MAP.md` (CLAUDE.md).
- [ ] 8.7 Cut the distributable beta (`make android-beta-dist`) and send it with
      **PRD Appendix B.2 verbatim**. The step order there is load-bearing: the
      File Selection Test must run **before** the log is copied, or the user sends
      a log with nothing in it and the round trip is wasted.
- [ ] 8.8 Add any question discovered during implementation to PRD §11 rather
      than resolving it silently.

---

## After the report comes back

Phase 2 is **not** planned in this document on purpose. Take the returned
`FILE-SELECTION-TEST:` block to PRD §4A.5, read off the row, and generate the
phase-2 task list from the requirement subset that row names. If the first column
says the URL was **empty**, note that none of PRD §5's requirements is the fix,
and the next investigation is the Qt Android `FileDialog` → ARC picker mapping —
a different piece of work than the one §5 describes.
