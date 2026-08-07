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

**Decided and implemented** (`cpp/android_raw_pick.cpp`). Two implementation
notes worth keeping: the `std::function` overload of
`QtAndroidPrivate::startActivity` (`qandroidextras_p.h:205-208`) avoids
subclassing `QAndroidActivityResultReceiver` entirely, so there is no receiver
object whose lifetime must outlive the picker; and a check against the **6.11**
branch found the file dialog helper unchanged, so the upgrade will not fix this
bug and the measurement is still needed.

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

## Review findings (2026-08-07, after task 7.0) — carry these into task 8.5

### 9. A raw URI that `QUrl` rejects must still be read directly

The gap this closed was in the **one case the whole round trip exists for**.
On the raw-intent path the report's `PickerUrlFacts` are derived from
`QUrl(raw_uri)`, so when that conversion fails — §4A.5's first row, and the
mechanism finding 6 predicts — the branch is `Empty`, the provider probe never
ran, and the block said only "the URL is unusable". That is the point at which
the interesting question *starts*.

`Uri.parse` / `ContentResolver.openInputStream` take a plain string and never
needed a `QUrl` at all. `run_file_selection_test` therefore probes the **raw**
URI directly whenever the `QUrl` route did not already read it, under a separate
`raw_provider_*` prefix so the two probes are never confused. A raw URI that
opens and reads while `QUrl` rejects it turns "bypass the conversion" from a
hypothesis into a demonstrated fix for phase 2.

It is deliberately **not** read twice: when the `QUrl` round-trip preserved the
URL and the provider branch already read it, the block says
`raw_provider: (not re-read …)`. A provider read can stream over a network.

### 10. Smaller corrections made in the same pass

- **The Android gate is `Qt.platform.os === "android"`, not `is_mobile`.** On
  iOS the raw intent does not exist, and `is_mobile` would have sent that
  platform to an "unsupported-platform" outcome instead of the picker it does
  have.
- **`start_raw_document_pick()`'s two Android failure paths delivered nothing.**
  Failing to build the intent, or a JNI exception while building it, returned
  `false` without calling `raw_document_pick_result_c()` — and the caller has by
  then already armed the listener and disabled its button, so the run would
  never complete. Both now deliver a `no-intent` outcome. Only a delivered
  result completes a run; this is the same rule as the `cancelled` branch.
- **The JNI result handler cleared pending exceptions after delivering, not
  before.** Delivery crosses into Rust, and an exception left pending across
  that boundary makes the next JNI call misbehave.
- The on-screen outcome line is kept short (`(Details are in log.txt.)`)
  because it shares the fixed bottom area with four buttons — the clipping
  hazard task 8.4 checks for.

## Relevant Files

- `cpp/utils.cpp` — convert the 7 real failure `qWarning`s (finding 3); delete
  the two dead qrc functions (finding 2); add the `QStandardPaths::TempLocation`
  staging-root accessor for D-12.
- `cpp/utils.h` — remove the two dead declarations (`:19-20`); declare the new
  accessor.
- `cpp/android_raw_pick.{h,cpp}` — **new.** Launches our own
  `ACTION_OPEN_DOCUMENT` and reports the picker's URI as the **raw Java string**,
  before any `QUrl` exists (tasks 3.11-3.14). The **only** file including private
  Qt API (`QtCore/private/qandroidextras_p.h`), deliberately kept self-contained
  so the import path never depends on it and the whole diagnostic is deletable in
  one commit. Delivers through `raw_document_pick_result_c()` — **on every
  path, including the two Android early failures** (invalid intent, JNI
  exception), which originally returned `false` without delivering anything and
  would have left the button disabled forever.
- `CMakeLists.txt` — the new source file, and `Qt6::CorePrivate` linked on
  **Android only**. Not added to `${qt_modules}`, which is also handed to
  `cxx_qt_import_crate(QT_MODULES)` and resolves names through qmake.
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
  (D-8f/g, D-9). This is phase-2 code in its final location. Two defects found
  in the review pass and fixed: the capped read treated only `n < 0` as end of
  stream, so a provider returning `0` would have spun the loop forever on a
  worker thread holding the keep-screen-on lock; and one `?` in
  `query_openable_columns` returned without closing the cursor.
- `backend/src/picker_url.rs` — additionally carries `set_raw_pick_listener()`
  (a plain `fn()` hook, no state of its own), which `raw_document_pick_result_c()`
  calls after storing the outcome. This is what wakes the bridge; the alternative
  (polling `take_raw_pick()` on a timer) is explicitly rejected in task 6.3b.
- `bridges/src/sutta_bridge.rs` — the `#[qsignal] fileSelectionTestCompleted`
  (beside `storageDiagnosticsCompleted` at `:822`), the
  `run_file_selection_test(url: &QUrl)` invokable (modelled on `:3964`), and the
  `unsafe extern "C++"` declaration of the new C++ accessor (beside `:708-769`).
  Also `start_file_selection_test_raw_pick()` (the Android entry point), the
  shared `spawn_file_selection_test()` worker, `picker_url_facts_from()`, and
  `on_raw_pick_finished()` — the listener that turns the native activity-result
  callback into a finished report, reaching the singleton through a registered
  `CxxQtThread<SuttaBridge>` in `FILE_SELECTION_TEST_THREAD` rather than a global
  object pointer. `QUrl(raw)` is reproduced here (`QUrl::from(&QString)`,
  `TolerantMode`) so `picker_url.rs` stays Qt-free.
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
- **`cargo check`/`cargo test` on the developer machine do not compile
  `android_saf.rs` at all** — it is `#[cfg(target_os = "android")]`, so the whole
  JNI half of this feature can be edited without any compiler ever seeing it.
  Check it explicitly, without needing a full Android build:

  ```sh
  NDK=~/Android/Sdk/ndk/27.3.13750724
  BIN=$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin
  cd backend && \
    CC_aarch64_linux_android=$BIN/aarch64-linux-android27-clang \
    CXX_aarch64_linux_android=$BIN/aarch64-linux-android27-clang++ \
    AR_aarch64_linux_android=$BIN/llvm-ar \
    CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=$BIN/aarch64-linux-android27-clang \
    cargo check --lib --target aarch64-linux-android
  ```

  (`ring`'s build script is why the three compiler variables are needed.) The
  C++ half has the same hole — `cpp/android_raw_pick.cpp` is `#ifdef
  Q_OS_ANDROID` and its private-Qt include is invisible to the desktop build:

  ```sh
  $BIN/clang++ --target=aarch64-linux-android27 -std=c++17 -fsyntax-only -Wall \
    -I cpp -I $QT/include -I $QT/include/QtCore \
    -I $QT/include/QtCore/6.9.3 -I $QT/include/QtCore/6.9.3/QtCore \
    cpp/android_raw_pick.cpp
  ```

  with `QT=~/Qt/6.9.3/android_arm64_v8a`. Both pass as of 2026-08-07.
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

### 1.0 [x] Make the native half of the import path visible in `log.txt` (D-14)

**Specs to keep in mind.** This is the one task that improves the *existing*
release's diagnosability, independently of everything else — the reporting user
cannot run `adb`, and today every native failure in the import path is
logcat-only. Two mechanisms, addressing two different sources: **conversion**
for messages we author, **a message handler** for Qt's own. Do not treat them as
alternatives (findings 1–4). Nothing here changes control flow.

**Depends on:** nothing. **Blocks:** nothing — do it first because it is
self-contained and de-risks every later on-device observation.

- [x] 1.1 Convert the **five** `qWarning`s in `copy_content_uri_to_temp_file`
      (`cpp/utils.cpp:648`, `:657`, `:664`, `:672`, `:683`) to `log_error_c`,
      using the established idiom at `:339`:
      `log_error_c(QString("…%1…").arg(x).toUtf8().constData())`. Preserve each
      message's text and every interpolated value (the `QFile::errorString()`
      calls especially — they are the native reason we are missing). Change **no**
      control flow, no return values, no early exits.
- [x] 1.2 Convert the **two** `qWarning`s in `copy_file` (`cpp/utils.cpp:565`,
      `:573`) the same way. These already build a `ret_msg` `QString`, so the
      conversion is `log_error_c(ret_msg.toUtf8().constData())`.
- [x] 1.3 Delete the two **dead** functions `list_qrc_assets()`
      (`cpp/utils.cpp:856`) and `copy_qrc_app_assets_to_internal_storage()`
      (`:870`), and their declarations at `cpp/utils.h:19-20`. Verified callable
      from nowhere in the tree (finding 2). Re-run the grep before deleting, in
      case the tree has moved on. If anything does call them, **stop and leave
      them alone** — do not convert their 11 tracing `qWarning`s, which would
      spam the log file the user emails.
- [x] 1.4 Leave `cpp/global_hotkey_x11.cpp`'s three `qWarning`s alone. They are
      X11-only, off the import path, and outside this feature's reason to touch
      the file.
- [x] 1.5 Install a `qInstallMessageHandler` in `cpp/gui.cpp`, after
      `init_app_globals()` (`:403`) and before `QApplication` (`:493`) — the
      slot the render-loop and palette pre-reads already occupy, and late enough
      that the logger's data dir is resolvable. Map `QtWarningMsg` →
      `log_error_c`, `QtCriticalMsg` / `QtFatalMsg` → `log_error_c` with the
      severity in the text, and `QtInfoMsg` → `log_info_c`.
- [x] 1.5a **Drop `QtDebugMsg` entirely.** Qt's debug stream is high-volume and
      would bury the `FILE-SELECTION-TEST:` block in the file we are asking the
      user to paste.
- [x] 1.5b Prefix every handler-routed line distinctly (e.g. `Qt: `) so a
      maintainer reading `log.txt` can tell a Qt-internal warning from one of the
      app's own messages. Include `context.category` when it is set — QML engine
      warnings carry `qml`, which is exactly the category PRD §2.1a's hypothesis
      would surface under.
- [x] 1.5c Keep the handler body **free of any Qt call that could itself warn**,
      and do not re-enter Qt logging from inside it. Compose the string with
      `QString`/`QByteArray` only and hand it to `log_*_c`. A handler that warns
      while handling a warning recurses until the stack is gone.
- [x] 1.5d Chain to the previous handler returned by `qInstallMessageHandler`, or
      deliberately do not, and **write which and why in a comment**. Not chaining
      means logcat loses Qt's warnings on Android (they now go to `log.txt`
      instead); chaining means they appear twice on desktop stderr. Recommended:
      chain, so `adb logcat` and Qt Creator's Application Output are unaffected
      and this task is purely additive.
- [x] 1.6 Build (`make build -B`) and verify on the developer machine that a Qt
      warning reaches `log.txt` — triggering any existing `qWarning` path is
      enough (metric 6). Confirm the app still starts and the log is not flooded
      (1.5a).

---

### 2.0 [x] `backend/src/picker_url.rs` — the Qt-free URL classifier

**Specs to keep in mind.** D-10 and PRD Req. 29: the branch decision must be a
**pure Rust function over strings**, so the ChromeOS behaviour we cannot
reproduce locally is still unit-testable here. The Qt side extracts the facts;
this module decides. Critically, **D-8(a) is measured first** — an empty URL is
the only failure the bug report actually demonstrates (PRD §2.1), and the module
must model it as a first-class branch rather than as "some other scheme".

**Depends on:** nothing. **Blocks:** 3.0, 4.0, 5.0.

- [x] 2.1 Create `backend/src/picker_url.rs`, declare it in `backend/src/lib.rs`,
      and define `PickerUrlFacts` — the Qt-extracted inputs, all owned `String`s
      plus one `bool`: `is_valid`, `encoded` (`to_encoded()`), `decoded`
      (`to_qstring()`), `scheme`, `host`, `local_file` (`toLocalFile()`).
      Keeping the struct Qt-free is what makes the tests possible.
- [x] 2.2 Define `PickerBranch` — `Empty`, `LocalFile`, `Provider { scheme }`,
      `BarePath` — and `classify(&PickerUrlFacts) -> PickerBranch`, mirroring
      PRD Req. 4(a)–(d) plus the new empty case. `Empty` when `!is_valid` **or**
      `encoded` is empty; `LocalFile` for `file`; `BarePath` for an empty scheme
      on a non-empty string; `Provider` for every other non-empty scheme
      (`content`, `externalfile`, anything else) — Req. 4(c) deliberately does
      not allowlist schemes.
- [x] 2.2a Do **not** treat a Windows drive letter as a scheme. `C:/Users/…`
      parses with scheme `c` in some URL libraries; the classifier works from
      Qt's already-parsed `scheme` field, so this is a **test** to write rather
      than logic to add (PRD §9.6 makes the same point for Req. 15).
- [x] 2.3 Add `encoding_differs(&PickerUrlFacts) -> bool` comparing `encoded`
      against `decoded`. This single boolean **is** the Defect B measurement
      (D-8c): a difference proves the corruption, identity rules it out for that
      pick. Keep it a named function so the report and the tests share one
      definition.
- [x] 2.4 Add a process-global run counter (`AtomicU64`) and
      `next_run_number() -> u64` (D-11), so repeated presses produce
      distinguishable blocks.
- [x] 2.5 Unit-test `classify` and `encoding_differs` against: `file:///path`
      (Unix), `file:///C:/path` (Windows), `file://server/share/x.zip` (UNC — the
      host must survive, Req. 7a), `content://…/document/primary%3ADownload%2Ffoo.zip`
      (the `%3A`/`%2F` **preserved** in `encoded` and **decoded** in `decoded`,
      so `encoding_differs` is true), `externalfile://…`, a bare
      `/home/user/x.zip`, a bare `C:/Users/x.zip`, and — the case that matters —
      an **empty** URL both as `is_valid: false` and as an empty `encoded`
      (metric 5).

---

### 3.0 [x] The document-URI probe in `android_saf.rs` (D-8f/g, D-9)

> **3.1-3.9 complete. 3.10 first decided against the private API, then
> REVERSED on 2026-08-07 after checking qtbase's 6.11 branch — see the decision
> recorded there; 3.11-3.14 are in scope.**

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

- [x] 3.1 Split `attach()` (`backend/src/android_saf.rs:47`) into
      `attach_resolver(vm) -> (AttachGuard, ContentResolver)` and a thin
      `attach_tree(vm, tree_uri)` that calls it and then does `Uri.parse` +
      `getTreeDocumentId`. Keep `write_to_tree_uri` and `child_exists` behaviour
      **byte-identical** — this is a refactor, and their tests/behaviour are the
      regression surface.
- [x] 3.2 Add `probe_document_uri(uri: &str, cap_bytes: usize) -> DocumentProbe`
      taking the **fully-encoded** URI string (never a pretty-decoded one — the
      `to_encoded()` trap of `docs/android-file-saving-saf.md` applies identically
      to reading). It parses with `Uri.parse` and opens via
      **`ContentResolver.openInputStream`** — *not* `QFile` (D-9), which only
      works for `content://` through `QAndroidContentFileEngine` and would fail on
      exactly the non-`content://` scheme this probe exists to detect.
- [x] 3.3 Have `DocumentProbe` carry every field D-8(f)/(g) needs, each
      independently `Option`al so a partial failure still reports what it learned:
      `opened: bool`, `display_name`, `size`, `bytes_read`, `open_ms`,
      `read_ms`, and `error: Option<String>` naming **which** step failed (URI
      parse, resolver open, query, read).
- [x] 3.4 Resolve the display name via `OpenableColumns.DISPLAY_NAME` and the size
      via `OpenableColumns.SIZE`, in one cursor query, tolerating a null cursor
      and a missing column without failing the whole probe (PRD Req. 9's
      fallbacks; the sanitisation half of Req. 9 belongs to phase 2, which
      actually writes a file).
- [x] 3.5 **Cap the read** at `cap_bytes` (a few MB — 4 MB is ample) and read in
      fixed-size chunks, discarding the bytes. D-4 forbids staging anything, and
      the test must never pull a 200 MB archive across a Drive connection. Report
      `bytes_read` and stop cleanly at the cap; reaching the cap is a **success**,
      not a truncation error.
- [x] 3.6 Close the stream on **every** path, including the error paths, and
      write no file anywhere (D-4). There is nothing to `Drop`-guard because
      nothing is created — state that in a comment so a later reader does not add
      a cleanup guard for a file that does not exist.
- [x] 3.7 Time the open and the capped read separately (D-8g). §9.5's
      Drive-streaming concern is a latency question and this is the only place it
      is ever measured.
- [x] 3.8 Gate the whole probe with `#[cfg(target_os = "android")]` and provide a
      non-Android stub returning a `DocumentProbe` whose error says the platform
      has no provider-backed reader (PRD Req. 27), so the desktop build compiles
      and the desktop report reads honestly.
- [x] 3.9 Confirm `cd backend && cargo test` still passes and the write path is
      untouched in behaviour — 3.1 is the only edit to shipping code in this task.

**Raw-URI capture (review finding 6).** Without these, an empty-URL result tells
us only what the existing log already told us, and the round trip with the user
is wasted. Qt's helper destroys the raw string at
`qandroidplatformfiledialoghelper.cpp:48` before any app code runs, so the only
way to see it is to run our own picker intent.

- [x] 3.10 **DECISION — confirm before implementing 3.11–3.14.** This uses Qt
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

      **DECIDED 2026-08-07: use the private API.** An initial "no" was reversed
      once the assumption behind it was actually checked against qtbase's
      **6.11** branch — the release this project is upgrading to. Full reasoning
      and the terms of the reversal are in PRD §11 Q0a; the three facts are:
      (1) `qandroidextras_p.h` on 6.11 declares `QAndroidActivityResultReceiver`
      and all three `QtAndroidPrivate::startActivity` overloads with signatures
      identical to the 6.9.3 kit, its last commit being cosmetic — the upgrade
      is a non-event for this API; (2) `Qt6::CorePrivate` is an **interface**
      target (include paths only, verified at `Qt6CoreConfig.cmake:133`), so
      there is no new `.so`, no ABI-slice growth and no manifest change, and a
      future break would be a **compile error**, not silent misbehaviour, on
      deletable diagnostic code; (3) 6.11 does **not** fix the bug — the helper
      still does `m_selectedFile.append(QUrl(uri.toString()))` and still has zero
      `qWarning`s — so the upgrade is no substitute for measuring. The
      custom-Java route would cost an `<activity>` entry in
      `android/AndroidManifest.xml`, breaking metric 4's byte-identical check on
      the one file with a Chromebook-filtering history.

      **Keep the private include confined to the diagnostic.** Phase 2's import
      path must not come to depend on it.
- [x] 3.11 Add an Android-only "raw pick" path: build an `ACTION_OPEN_DOCUMENT`
      intent with `CATEGORY_OPENABLE` and `setType("*/*")` (no MIME filter — the
      D-3 rationale applies here too), and launch it with
      `QtAndroidPrivate::startActivity` using a **request code that cannot collide
      with Qt's own `1305`** (`qandroidplatformfiledialoghelper.cpp:24`).
- [x] 3.12 In the result receiver, read `intent.getData()` and record
      **`uri.toString()` as a raw Java string**, before any `QUrl` exists. Also
      record `QUrl(uri.toString()).isValid()` — the single most valuable line in
      the whole report, because it reproduces Qt's `:48` conversion and shows
      directly whether that is where the URL is lost. Handle the `getClipData()`
      branch (`:57-69`) too, and the "neither" case, which Qt leaves silently
      emitting nothing.
- [x] 3.13 Feed the raw string into the **same** `PickerUrlFacts` pipeline (task
      2.1) so both paths produce the same report shape, with one line naming which
      path produced the block (Qt `FileDialog` vs raw intent). Do **not** fork the
      report builder.
- [x] 3.14 Gate all of it behind `#[cfg(target_os = "android")]` / `#ifdef
      Q_OS_ANDROID` with a desktop stub, and confirm the desktop build neither
      links nor references the private header (PRD Req. 27).

---

### 4.0 [x] Import-staging facts (D-12)

**Specs to keep in mind.** This section needs **no user interaction at all** — it
is pure measurement that settles Defect D (PRD §2.5), which the PRD currently
calls "very likely a silent no-op". The comparison is the point: the C++ writer
uses `QStandardPaths::TempLocation` (`cpp/utils.cpp:645`) and the Rust cleanup
uses `std::env::temp_dir()` (`bridges/src/sutta_bridge.rs:3687`). Print both and
say plainly whether they differ.

**Depends on:** 2.1 (the module). **Blocks:** 5.0.

- [x] 4.1 Add a C++ accessor returning the staging root
      (`QStandardPaths::writableLocation(QStandardPaths::TempLocation) + "/simsapa-imports"`)
      to `cpp/utils.cpp` + `cpp/utils.h`, and declare it in the
      `unsafe extern "C++"` block of `bridges/src/sutta_bridge.rs` beside
      `get_android_package_name()` (`:708-709`). **Derive it from the same
      expression `copy_content_uri_to_temp_file` uses** — ideally by extracting
      that expression into the new function and calling it from both, so the two
      cannot drift.
- [x] 4.2 In `picker_url.rs`, add `collect_staging_facts(cpp_root: &str) -> StagingFacts`
      recording: the C++ root (passed in — the backend stays Qt-free), the Rust
      root (`std::env::temp_dir().join("simsapa-imports")`), and an explicit
      `roots_differ: bool`. The bridge supplies `cpp_root`; do not try to reach
      Qt from the backend.
- [x] 4.3 Census the staging folder, tolerating its absence as a normal reported
      fact rather than an error: exists (`try_exists()`), file count, total size,
      and the age of the oldest entry — the evidence for or against PRD Req. 21a's
      unbounded-footprint claim.
- [x] 4.4 Report free/total space on the staging volume via **`fs4::statvfs`**, matching
      `storage_diagnostics.rs:282` (finding 8) — already a direct dependency
      (`backend/Cargo.toml:53`), cross-platform, no new code per platform. This is the input PRD Req. 24's threshold will need.
- [x] 4.5 Census **both** roots when they differ, not just the C++ one. The whole
      point is to show which directory actually holds the staged files and which
      one the cleanup is pointed at; reporting only one cannot demonstrate the
      mismatch.
- [x] 4.6 Unit-test `collect_staging_facts` against a temp directory with known
      contents, and against a non-existent root (must report cleanly, not error).

---

### 5.0 [x] The report builder and the `run_file_selection_test()` entry point

**Specs to keep in mind.** The deliverable of this whole feature is a block of
INFO lines in `log.txt` (D-7) — there is no results window (§4A.4). Every line
carries the `FILE-SELECTION-TEST:` prefix so it is greppable and survives a
truncated paste, and every block carries a run number and timestamp (D-11) so
repeated picks from Downloads / Play files / Drive are distinguishable. **D-8(a)
first, and never stop at the first blank**: if the URL is empty, say so and keep
reporting whatever else is knowable, because "empty" is the finding.

**Depends on:** 2.0, 3.0, 4.0. **Blocks:** 6.0.

- [x] 5.1 Implement
      `run_file_selection_test(facts: &PickerUrlFacts, cpp_staging_root: &str) -> String`
      in `picker_url.rs`, returning the whole block as a `String` (the pattern
      `storage_diagnostics::run_storage_diagnostics()` follows, and what makes it
      unit-testable).
- [x] 5.2 Emit the header: run number (2.4), timestamp, and the platform — plus
      Android API level where applicable, reusing `storage_diagnostics.rs`'s `current_platform()` (`:1875`) and
      `android_api_level()` (`:1892`) rather than writing a second copy — **both are
      private today and must be made `pub`** (finding 8).
- [x] 5.3 Emit the URL lines in D-8's order, one labelled line each: **(a)
      empty/invalid first**, then encoded, decoded, an explicit
      `encoding_differs: yes/no` line (2.3), scheme, host, path segment count.
      Where the URL is empty, emit the empty verdict and then continue to the
      staging facts — the block must never end early (D-8a).
- [x] 5.3a **Emit the raw-pick lines (D-8h), and emit them before the `QUrl`
      lines** — they are upstream of everything else, and PRD §4A.5's
      raw-intent rows are read first. From `take_raw_pick()` (already
      implemented in `picker_url.rs`): the **raw URI string exactly as the
      picker returned it**, its length, the branch that produced it
      (`intent-getData` / `intent-getClipData` / `cancelled` / `no-uri` /
      `no-intent` / `unsupported-platform`), and a `PickSource` line naming
      which picker the block came from (D-3a).
      **This is the whole deliverable of the round trip.** Today the native
      side logs only the URI's *length* (`android_raw_pick.cpp`) and the Rust
      callback logs only `source` + length — the string itself sits in
      `RAW_PICK_RESULT` waiting for this task. If 5.3a is skipped or emits only
      a summary, the user's log comes back with nothing new in it and the round
      trip is wasted. PRD Req. 17 permits it: URLs and paths are acceptable in
      the log, file contents are not.
- [x] 5.3b Beside the raw URI, emit whether **`QUrl(raw)` is valid** — the line
      that reproduces `qandroidplatformfiledialoghelper.cpp:48` and decides the
      first two rows of the new §4A.5 table. It must use the same constructor
      Qt uses (`QUrl(QString)`, `TolerantMode`). **Verified 2026-08-07:**
      cxx-qt-lib's `QUrl::from(&QString)` resolves through
      `qurl_init_from_qstring` to exactly that constructor. Building the `QUrl`
      needs Qt, so this line is produced in `bridges/` (task 6.3a) and passed
      into the builder as a plain `bool` — the backend module stays Qt-free.
- [x] 5.4 For the `LocalFile` branch, emit the `toLocalFile()` path and its
      `try_exists()` result (D-8e, Req. 7a). Do **not** use `QUrl::path()`
      anywhere in this feature; it drops the host and silently breaks Windows UNC
      picks.
- [x] 5.5 For the `Provider` branch, call `probe_document_uri` (3.2) with the
      **encoded** URI and emit its fields, including the failing step on error
      (D-8f, D-8g).
- [x] 5.6 For `BarePath`, emit the path and its `try_exists()` — it should not
      occur from a picker, and saying so is how we would learn that it did.
- [x] 5.7 Append the staging facts (4.2–4.5) to every block, whatever the URL
      branch. They are independent of the pick, and a user who only ever produces
      empty-URL blocks still supplies them.
- [x] 5.8 Log the whole block through the Rust logger at **INFO** (D-7), in one
      call or in clearly contiguous lines that reassemble, and also return it so
      the bridge can put a one-line outcome on screen.
- [x] 5.9 Add a short `outcome_line(&PickerUrlFacts, …) -> String` producing the
      **plain-language** one-liner for D-6/D-13 — "The file picker did not return
      a file." for the empty case. It must not print `Path not found:`, and it is
      the wording model for phase 2's Req. 13, so keep it to one sentence a
      non-developer can act on.
- [x] 5.10 Assert by test that the block contains no file **contents** and no
      `api_key`-shaped text (PRD Req. 17: the URL and paths are acceptable, file
      contents are not) — 3.5 discards the bytes it reads, and this test guards
      that.
- [x] 5.11 Unit-test the builder end-to-end against fixture `PickerUrlFacts` for:
      empty URL, `file://` that exists, `file://` that does not, a `content://`
      with differing encoded/decoded forms, and an unknown scheme. Assert the
      prefix is on every line and the run number increments across calls.

---

### 6.0 [x] Bridge wiring: a `&QUrl`-taking invokable with a completion signal

**Specs to keep in mind.** CXX-Qt invokables run on the **calling (QML) thread**,
so the run must be spawned (D-5); `run_storage_diagnostics`
(`bridges/src/sutta_bridge.rs:3964`) is the pattern to copy, with its signal at
`:822-824`. The `QUrl` must be passed **as a `QUrl`**, never as a string from
QML — that is Defect B's whole lesson (PRD Req. 2), and `save_file(folder_url:
&QUrl, …)` (`:3388`) is the proven precedent. Any new `SuttaBridge` method
**and** signal needs a `qmllint` stub (PRD §8).

**There are now TWO ways a run starts, and only one of them is QML-initiated.**
D-3a splits the picker by platform, so the bridge needs two entry points that
converge on one report builder and one completion signal:

| Platform | Started by | Result arrives via |
|---|---|---|
| desktop | QML `FileDialog.onAccepted` → `run_file_selection_test(url)` | the invokable's own worker |
| Android | `start_raw_document_pick()` → the system picker | the **native activity-result callback**, `raw_document_pick_result_c()` |

The Android half has **no trigger today**: `raw_document_pick_result_c`
(`backend/src/picker_url.rs`) stores the outcome into `RAW_PICK_RESULT` and
returns, and nothing wakes the bridge to build a report. Task 6.3b adds it.
Do **not** solve this by polling.

**Depends on:** 5.0. **Blocks:** 7.0.

- [x] 6.1 Add `#[qsignal] #[cxx_name = "fileSelectionTestCompleted"]
      fn file_selection_test_completed(self: Pin<&mut SuttaBridge>, success: bool, outcome: QString)`
      beside `storage_diagnostics_completed` (`:822-824`), following the same
      convention. `outcome` carries the D-6 one-liner, not the whole block — the
      block goes to the log.
- [x] 6.1a **Verify `Pin<&mut Self>` + `&QUrl` compiles before building on it**
      (finding 8). There is no precedent in this codebase: every `&QUrl` method is
      `self: &SuttaBridge`, and every spawn-and-signal invokable takes no `&QUrl`.
      A one-line throwaway invokable settles it in one `make build -B`. If it does
      **not** compile, the fallback is a `&self` invokable that extracts the facts
      and hands them to a separate `Pin<&mut Self>` method — do **not** fall back
      to passing the URL as a `QString` from QML, which reintroduces the exact
      corruption being measured.
- [x] 6.2 Add `#[qinvokable] run_file_selection_test(self: Pin<&mut SuttaBridge>, url: &QUrl)`.
      Extract the `PickerUrlFacts` from the `QUrl` **on the calling thread**, using
      the cxx-qt-lib Rust names (finding 8): `is_valid()`, `to_encoded()`,
      `to_qstring()`, `scheme_or_default()`, `host_or_default()`,
      `to_local_file()` (which is `None` unless `is_local_file()`) — into owned
      `String`s, then move those
      into the worker. `QUrl` is not `Send`; extracting first is what makes the
      spawn legal, and it is also the moment the encoding is preserved.
- [x] 6.2a Recover the encoded form exactly as `save_bytes_to_folder` does at
      `:614`: `String::from_utf8_lossy(url.to_encoded().as_slice()).to_string()`.
      Do **not** use `to_qstring()`, `to_display_string()` or `path()` for it —
      those are the pretty-decoded forms and are only captured separately, as the
      *measurement* of D-8(c).
- [x] 6.3 Fetch the C++ staging root (4.1) on the calling thread too, and pass the
      resulting `String` into the worker — the same reason as 6.2.
- [x] 6.3a Build the D-8h `QUrl(raw)` reproduction here, where Qt is available:
      `QUrl::from(&QString::from(raw_uri))` then `is_valid()`, passed into the
      report builder as a plain `bool` (task 5.3b) so `picker_url.rs` stays
      Qt-free. Use **that** constructor and no other — it is the one Qt's file
      dialog helper uses, verified 2026-08-07 to resolve through
      `qurl_init_from_qstring` to `QUrl(QString)` in `TolerantMode`. A
      strict-mode parse would answer a different question and quietly
      mis-diagnose the bug.
- [x] 6.3b **Add the Android entry point** — an invokable
      `start_file_selection_test_raw_pick()` that calls the already-declared
      `start_raw_document_pick()` (`sutta_bridge.rs`, from `android_raw_pick.h`),
      plus the path that turns the native callback into a finished report.
      Design constraints, in order of how easy they are to get wrong:
      - the callback runs on the **Android UI thread**, inside the activity
        result dispatch. Do the report there and the UI stalls for the length of
        a provider read; so hand off to a worker exactly as 6.4 does, and queue
        the completion through `qt_thread()`;
      - the callback is a plain `extern "C"` function with **no `self`**, so it
        cannot reach the `SuttaBridge`. Give it a registered
        `CxxQtThread<SuttaBridge>` (captured when the pick is started) rather
        than reaching for a global `SuttaBridge` pointer;
      - a `cancelled` outcome must still complete the run — release the
        keep-screen-on lock and re-enable the button (task 7.4) — or the button
        stays dead until the dialog is reopened;
      - **do not poll** `take_raw_pick()` on a timer. The callback is the event.
- [x] 6.4 Spawn the worker with `catch_unwind` (copying `:3966-3988`), emit
      `success: false` with the panic message rather than losing the signal, and
      queue the completion back through `qt_thread()`.
- [x] 6.5 Add the `qmllint` stubs to
      `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`: the signal beside
      `storageDiagnosticsCompleted` (`:47`) and a trivial
      `run_file_selection_test(url: url)` function stub beside the other bridge
      methods.
- [x] 6.6 `make build -B` and confirm the generated QML type exposes both the
      method and the signal.

---

### 7.0 [x] QML: the "File Selection Test" button and the picker flow

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

**One press opens exactly one picker, and which one depends on the platform**
(PRD D-3a, decided 2026-08-07): desktop opens Qt's `FileDialog`; **Android does
not** — it calls `SuttaBridge.start_file_selection_test_raw_pick()` (task 6.3b)
and never instantiates the `FileDialog` at all. Opening both would put two
consecutive pickers in front of the user and contradict Appendix B.2, which tells
them to pick the file once. The `FileDialog` therefore needs a platform gate even
though the *button* does not (D-2).

**Depends on:** 6.0. **Blocks:** 8.0.

- [x] 7.1 Add the **"File Selection Test"** button to the `ColumnLayout` at
      `AboutDialog.qml:255-291`, between "Copy App Info" and "Run Storage
      Diagnostics", with `Layout.fillWidth: true` like its siblings. **No
      platform gate** — D-2 wants it on desktop too, where it exercises the
      `file://` branch a maintainer can actually read.
- [x] 7.1a Branch the button's `onClicked` on the platform (D-3a): on Android
      call `SuttaBridge.start_file_selection_test_raw_pick()` (6.3b); everywhere
      else open the `FileDialog` of 7.2. Use the dialog's existing
      `root.is_desktop`-style gate rather than inventing a second one. Log which
      path was taken (D-7) — a block whose picker is ambiguous cannot be read
      against PRD §4A.5, whose Android rows apply only to the raw-intent path.
- [x] 7.2 Add a `FileDialog` with **no `nameFilters`** (D-3) and a title naming
      the purpose. On `onAccepted`, pass `selectedFile` **straight** into
      `SuttaBridge.run_file_selection_test(selectedFile)` — no `String(...)`, no
      `strip_file_scheme`, no JavaScript inspection of the URL whatsoever
      (PRD Req. 2). Any QML-side string handling would re-introduce the very
      corruption being measured.
- [x] 7.2a Log the QML-side view of the pick before the call, through
      `Logger { id: logger }` (already at `:14`) with a **single concatenated
      string** (D-7): the run being started, and — because this is the case under
      investigation — whether `selectedFile` is empty as QML sees it, plus
      `selectedFiles.length` and the dialog's `currentFolder`. If Qt handed QML
      nothing, this is the line that proves it independently of the Rust side.
      **Expect all of them to be empty together** — they are fed from one
      `m_selectedFile` list inside Qt (finding 6), so they corroborate rather than
      recover. The raw URI comes from tasks 3.11-3.13, not from here.
      **Desktop only now** (D-3a) — on Android this dialog never opens, so these
      lines will not appear in an Android block and their absence is not a fault.
- [x] 7.2b Handle `onRejected` by logging a cancelled test, so a user who backs
      out of the picker does not leave a maintainer wondering whether the button
      worked. The Android equivalent is the `cancelled` branch of the native
      callback (6.3b), which must complete the run rather than leave the button
      disabled.
- [x] 7.3 Give `AboutDialog` its own `AssetManager { id: manager }` and bracket
      the run with `set_keep_screen_on(true)` before the invokable and `(false)`
      in the completion handler on **both** success and failure (D-5). Finding 5:
      the storage-diagnostics task list's instruction *not* to add an
      `AssetManager` here applied to a run owned by its results window and does
      not apply to this feature. **Extend the comment at `AboutDialog.qml:31-34`**
      to say so, or the new `AssetManager` reads as a contradiction of it
      (finding 8).
- [x] 7.4 Add a `Connections` on `SuttaBridge` handling
      `onFileSelectionTestCompleted` — set the on-screen outcome text, re-enable
      the button, release the keep-screen-on lock. Guard with an "initiated here"
      boolean: the signal is process-global, and although `AboutDialog` is
      currently the only listener, the guard is what keeps that safe when
      Appendix B's step 4 has the user run it repeatedly.
- [x] 7.5 Show the outcome **on screen** (D-6): ~~a single wrapping `Label` under
      the button~~ **a `MessageDialog` with a Close button**, carrying the 5.9
      one-liner plus a fixed reminder that the detail is in the log file listed
      above. Disable the button and show a busy state while the run is in flight
      (D-5).

      **Changed 2026-08-07 after the first on-device run.** The Label was built
      as specified and worked, but it sits *inside* the fixed bottom
      `ColumnLayout` with four full-width buttons, so a wrapping outcome grows
      that column downwards and pushes "Close" toward the screen edge — the
      clipping hazard 8.4 was written to watch for, arriving through the outcome
      line rather than through the fourth button. A `MessageDialog` (the idiom
      already in this file, `save_log_msg_dialog`) takes the text out of the
      layout entirely, and the explicit acknowledgement is better suited to a
      user who has to report what they saw. The one-liner is now the dialog's
      `text` and the log-file reminder its `informativeText`, so the outcome no
      longer has to be kept artificially short.
- [x] 7.6 Ensure the empty-URL case reads in **plain words** — "The file picker
      did not return a file." — and never surfaces `Path not found:` (D-13).
- [x] 7.7 Confirm **no new QML file** was created, so `bridges/build.rs` needs no
      change. If a component is factored out later, it must be added to
      `qml_files` (CLAUDE.md) — but D-6 does not want one.
- [x] 7.8 Run `make qml-test`; confirm `qmllint` is clean and no `console.*` call
      was introduced (D-7).

---

### 8.0 Tests, non-goal verification, docs, and the build to send

**Specs to keep in mind.** Phase-1 metrics 4–6 are the ones an implementer can
get wrong without noticing: **no import behaviour may change**, the manifest must
be byte-identical, and the four call sites of PRD §2.6 must be untouched. The
feature is only useful if the user's log comes back readable, so the doc and the
email are part of the deliverable, not an afterthought.

**Depends on:** 1.0–7.0.

- [x] 8.1 Consolidate the unit tests from 2.5, 4.6, 5.10 and 5.11 and confirm
      `cd backend && cargo test` and `make qml-test` pass (metric 5). Record any
      pre-existing timing-assertion drift separately rather than as a regression.

      **Run 2026-08-07:** `cargo test` — 0 failed across every binary, of which
      **32** are `picker_url::tests` (2.5, 4.6, 5.10, 5.11 all live in the one
      `mod tests` at the bottom of `backend/src/picker_url.rs`, the
      `storage_diagnostics.rs` convention, so no consolidation was needed).
      `make qml-test` — 131 passed, 0 failed. No timing-assertion drift fired on
      this run.
- [x] 8.2 Verify the classifier covers every §4A.5 decision-gate row, so each
      possible report lands in exactly one of them (metric 2). Add a test per row
      if any is unreachable from the fixtures already written.

      All nine rows are reachable; **six** were only *implied* by the fixtures and
      now have a named `gate_*` test each (`picker_url.rs`, 32 → 38 tests). The
      gaps closed: QUrl row 3 asserted only the scheme, never the "identical
      encoding" half the row turns on; row 4's *true* complement was untested, so
      a working desktop pick could be misread as the scoped-storage finding; row 5
      had no test at all; raw row B never asserted `qurl_of_raw_is_valid: yes`;
      raw row C existed only as an `outcome_line` assertion, not a block; raw
      row D covered `no-uri` but not `no-intent` — which is what the two Android
      early-failure paths deliver.

      **Row 5 is the one row not fully reachable off-device**, and deliberately
      so: it needs `provider_opened: true`, which only the JNI
      `probe_document_uri` can produce (the desktop stub always reports "no
      provider-backed reader"). It is split — `classify` + `encoding_differs` are
      covered by `a_short_phone_content_uri_may_encode_identically`, and the
      *rendering* of a successful probe is covered by feeding `append_probe` a
      constructed `DocumentProbe`. So the row is readable when it arrives from a
      phone; only the read itself is untestable here, which is the same
      structural limit Req. 29 accepts throughout.
- [x] 8.3 Diff the branch against the §4A.4 non-goals and confirm, explicitly:
      `DictionaryImportDialog.qml`, `DocumentImportDialog.qml`,
      `ChantingPracticeWindow.qml` and `GlossTab.qml` are **untouched**;
      `strip_file_scheme` and `file_url_to_path` still exist unchanged;
      `android/AndroidManifest.xml` is **byte-identical**; no network call was
      added; nothing is staged or written into the import folders (D-4); and
      `bridges/build.rs` is unchanged (metric 4).

      **Verified 2026-08-07** against `git merge-base main HEAD`
      (`d7cdd77`). The branch touches **15 files**, and the four call sites are
      not among them:
      - `DictionaryImportDialog.qml`, `DocumentImportDialog.qml`,
        `ChantingPracticeWindow.qml`, `GlossTab.qml` — all `git diff --quiet`
        clean. `strip_file_scheme` (`DictionaryImportDialog.qml:88`) and both
        `file_url_to_path` copies (`ChantingPracticeWindow.qml:60`,
        `GlossTab.qml:815`) still exist, unchanged.
      - `android/AndroidManifest.xml` — **byte-identical**, confirmed by
        sha256 against the merge-base blob, not merely by `git diff`
        (`7d7f0590…` both sides).
      - `bridges/build.rs`, `dictionary_manager_core.rs`,
        `dictionary_manager.rs` — untouched.
      - **D-4:** every write-shaped call (`fs::write`, `create_dir`,
        `File::create`, `OpenOptions`, `remove_*`) in `picker_url.rs` falls
        **after** `mod tests` at `:882`; production code writes nothing. The
        `android_saf.rs` probe uses `openInputStream` only — no
        `openOutputStream`, no `createDocument`. Corroborated on device by the
        run under 8.4: `staging_cpp_exists: false` *after* a completed run.
      - **No network call** introduced anywhere in the diff.
      - Incidental changes are all accounted for: `storage_diagnostics.rs` is
        two `fn` → `pub fn` and nothing else (finding 8); `CMakeLists.txt` adds
        the new source and the Android-only `Qt6::CorePrivate` interface target;
        `cpp/utils.cpp` is −73/+22, being the 7 conversions plus the two dead
        qrc functions, which are now referenced from nowhere in the tree.

      **One deliberate leftover:** 4 `qWarning`s remain in `cpp/utils.cpp`, all
      in `copy_apk_assets_to_internal_storage` (`:809`, `:825`, `:848`) — a
      different function from the two task 1.0 scoped, so converting them was
      not this feature's business. They now reach `log.txt` anyway through the
      task-1.5 message handler, which is the outcome that mattered.
- [x] 8.4 **On-device check by the user, not the agent** (CLAUDE.md): build
      `make android-beta-debug`, install, and confirm on a phone that the button
      appears in the right position, the picker opens with **no** file-type
      filter, a normal `content://` pick produces a complete block in `log.txt`,
      and the four buttons all fit without clipping — the defect the storage
      diagnostics work hit at its task 8.7a, now with a fourth button. The
      outcome no longer shares that area (7.5 moved it into a `MessageDialog`),
      so the checks are: the four buttons fit, and the result dialog appears,
      reads plainly, and closes on its Close button.

      **Partly done 2026-08-07 on a Samsung SM-S911B (Android 16, API 36)** —
      *not* a Chromebook, so this run validates the **instrument**, not the bug.
      Confirmed: the button is in the right position, the raw
      `ACTION_OPEN_DOCUMENT` picker opens unfiltered, and one press produced a
      complete, well-formed block (run 1) end to end — `raw_branch:
      intent-getData`, the raw URI captured at full length, `provider_opened:
      true`, `display_name`/`size` resolved, 3518 of 3518 bytes read,
      `open_ms: 29` / `read_ms: 1`, staging census and free space all present,
      and the `raw_provider: (not re-read …)` suppression firing correctly. The
      **Re-tested 2026-08-07 after the 7.5 dialog change** (same device), two
      runs in one session:
      - **run 1, normal pick** — as above, `open_ms: 11` / `read_ms: 0`; outcome
        *"The file picker returned a file from another app (scheme: content)."*
      - **run 2, cancelled pick** — the path that had to be checked, because a
        cancel that does not complete the run leaves the button dead until the
        dialog is reopened. It completes: `raw_branch: cancelled`, the run
        counter increments, the staging facts are still reported (they do not
        depend on the pick), and the block is unambiguously **not** the
        empty-URL finding — `url: (no URL to examine on this run)`, which is
        what §4A.5's cancelled row needs to stay distinguishable. This confirms
        `gate_raw_row_c` (8.2) on hardware.

      User-confirmed visually: the four buttons fit with nothing clipped now the
      Label is gone, and the result dialog reads correctly and closes on its
      Close button. No QML errors, no `TypeError`, and no Qt-routed warnings in
      the session (the two `Failed to fetch releases info … using embedded
      fallback` lines are documented offline behaviour, unrelated).

      **Two measurements contradict PRD assumptions — carry to 8.8:**
      - `staging_roots_differ: **no**`. Both roots are
        `/data/user/0/…/cache/simsapa-imports`. PRD §2.5 Defect D.2 asserts the
        C++ `QStandardPaths::TempLocation` and Rust `std::env::temp_dir()` are
        "not the same directory" on Android, making the cleanup "very likely a
        silent no-op". On this device they are identical, so **Req. 20 may be a
        non-issue** and Req. 21a's unbounded-footprint claim loses its D.2 half.
        Measured on one device / one Android version — not yet general.
      - `encoding_differs: **no**`, with `%3A` and `%2F` preserved in **both**
        forms. Exactly what review finding 8 predicted: Qt's `toString()`
        defaults to `PrettyDecoded`, which does not decode a delimiter inside a
        path. This is **data, not a defect in the report** — see finding 8's
        standing instruction not to "fix" the builder when the two come back
        identical.
- [x] 8.5 Write `docs/file-selection-test.md`: what each D-8 line means, how to
      read a `FILE-SELECTION-TEST:` block, the §4A.5 decision-gate table, the
      note that the `android_saf.rs` probe is phase-2 code wired only into the
      diagnostic (task 3.0), and the finding-2 record that the two qrc functions
      were dead when deleted.

      Written. Beyond the required content it carries §3 (the two-picker split
      and why Android bypasses Qt's `FileDialog`, with the `:48` mechanism),
      §3.1 (the private-Qt include and the terms it is confined by), §7 (the
      never-do list) and §8 (the message handler, including the correction that
      it does **not** catch this bug). **§6 is the one to read before touching
      the report**: it names four measured states that are normal and must not
      be "fixed" — `encoding_differs: no`, `staging_roots_differ: no`,
      `staging_cpp_exists: false` and `provider_reached_cap: true` — with the
      first two backed by the 8.4 device run and flagged as contradicting PRD
      §2.3 and §2.5 respectively.
- [x] 8.6 Add the **read**-path cross-reference to
      `docs/android-file-saving-saf.md` so the `to_encoded()` rule is stated for
      both directions (PRD §8), and update `CLAUDE.md`'s notable-docs list and
      `PROJECT_MAP.md` (CLAUDE.md).

      - `docs/android-file-saving-saf.md` gains a **"The read path — the same
        rule, the other direction"** section: the write/read function table, the
        `attach()` → `attach_resolver` + `attach_tree` split and why a document
        URI forced it, and the `ContentResolver`-not-`QFile` rule. It opens by
        naming the read side as where the `to_encoded()` rule was *missing* —
        which is one of the defects behind the Chromebook failure.
      - `CLAUDE.md` — **note it is a symlink to `AGENTS.md`**; edit the target.
        The SAF entry gains the read-path sentence, and a full
        **File Selection Test** entry was added after it.
      - `PROJECT_MAP.md` — three additions: `src/picker_url.rs` in Key Modules,
        an "Android SAF reader" bullet beside the writer, a "File Selection Test"
        bullet listing every file involved, and the `AboutDialog.qml` line now
        records that it **owns** this run (unlike the storage diagnostics) and
        that its button area must stay a `ColumnLayout`.
- [ ] 8.7 ~~Cut the distributable beta (`make android-beta-dist`)~~ **Release to
      Google Play closed testing** and send it with **PRD Appendix B.2
      verbatim**. The step order there is load-bearing: the File Selection Test
      must run **before** the log is copied, or the user sends a log with nothing
      in it and the round trip is wasted.

      **Distribution changed 2026-08-07 to Play closed testing.** The
      sideloaded beta APK was never really available to this user: unknown-source
      installs in ARC normally need the Chromebook in **developer mode** (a
      powerwash), and a managed device may forbid it outright. Consequences:
      - the artifact is the **AAB** from `make android-aab`, package
        `io.github.simsapa.app`. `make android-beta-dist` and the
        `io.github.simsapa.app.beta` package play no part;
      - **it is not a second app** — a closed track ships the same package, so
        joining updates their existing Simsapa and leaving reverts it. Any
        "installs alongside" wording is wrong;
      - `android/version.txt` (currently **6**; production on the test phone is
        **5**) must exceed every versionCode *ever uploaded*, including ones
        never promoted — check the Play Console before uploading;
      - build with `make android-aab`, **never from the Qt Creator interface**:
        its kits are single-ABI and an arm64-only bundle is filtered off
        Intel/AMD Chromebooks, which is this user's device;
      - we now need the user's Google account address to add them as a tester,
        and the turnaround includes review time — do not read silence as failure.

      **Draft email: `tasks/2026-07-31-180502-email-to-reporting-user.md`**, with
      B.2's four steps verbatim, the B.3 environment questions (the picker one
      carrying its 2026-08-07 note), and the notes-for-us section. Still to do:
      the versionCode check, the upload, the tester link, and sending it.
- [x] 8.8 Add any question discovered during implementation to PRD §11 rather
      than resolving it silently.

      Three added as **§11 Q4-Q6**, all from running the finished diagnostic on
      the Samsung test phone (Android 16) — the instrument's bench, not the
      reported platform, so each is a question about the PRD's *premises*, not
      an answer about the bug:
      - **Q4 — is Defect D.2 real on any device?** The two staging roots are
        **identical** on Android 16. If that holds on ARC, Req. 20 is a
        non-issue and Req. 21a loses its D.2 half (its "nothing ever deletes the
        staged copy" half is unaffected). The returning Chromebook block carries
        these lines, so it answers this for free.
      - **Q5 — does `encoding_differs` ever come back `true`?** A deeply encoded
        `content://` pick was **byte-identical** in both forms. Defect B may be
        milder than §2.3 states.
      - **Q6 — do Qt's `FileDialog` and a plain `ACTION_OPEN_DOCUMENT` reach the
        same picker on ChromeOS?** The accepted cost of D-3a, written down so it
        is not rediscovered as a surprise when the report arrives.

      §2.5 and §2.3 also carry short in-place notes pointing at Q4 and Q5, so a
      reader of the background sections is not left with the unqualified claim.
      Both notes are careful to say what still stands: §2.5's claim 1 (an
      unrelated import wiping the shared folder) is untouched, and §2.3's defect
      is real on the write path it cites.

---

## After the report comes back

Phase 2 is **not** planned in this document on purpose. Take the returned
`FILE-SELECTION-TEST:` block to PRD §4A.5, read off the row, and generate the
phase-2 task list from the requirement subset that row names. If the first column
says the URL was **empty**, note that none of PRD §5's requirements is the fix,
and the next investigation is the Qt Android `FileDialog` → ARC picker mapping —
a different piece of work than the one §5 describes.
