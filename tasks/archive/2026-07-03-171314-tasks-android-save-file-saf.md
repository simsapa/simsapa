# Tasks: Android "Save As…" Silently Fails (Storage Access Framework)

Fixing the bug reported 2026-07-03: on Android, AboutDialog → **Save As…** for
`log.txt` completes with no visible error, the "Allow … to access folder?"
prompt is answered *Allow*, the dialog closes — but the file is never written,
and nothing is logged (neither in-app nor in the Qt Creator debug console).

The same defect affects **every** `SuttaBridge.save_file(...)` caller (Gloss /
Prompts / Gloss-tab multi-file export), not only the log save. All of them are
broken on Android and silently report success.

## Issue Descriptions

### Issue A — `save_file` reports success unconditionally (silent failure)

`SuttaBridge::save_file` (`bridges/src/sutta_bridge.rs:2755`) calls the backend
`save_to_file` (`backend/src/lib.rs:1145`) but **discards its return value** and
returns `true` whenever `output_path.to_str()` is `Some`:

```rust
match output_path.to_str() {
    Some(p) => {
        save_to_file(content.to_string().as_bytes(), p);  // Result string dropped
        true                                              // always true
    },
    None => false,
}
```

`save_to_file` returns a `String` describing success **or** failure
(`"Failed to create file: …"`), but the caller ignores it. Consequently the QML
guards — `AboutDialog.qml:285` `if (!ok) { logger.error(...) }`, and the
"Exported as…" success dialogs in `GlossTab.qml` / `PromptsTab.qml` — never see
a failure. That is why the reporter saw **no error anywhere**: the write failed,
but the whole stack claimed it succeeded.

This is a real bug independent of Android (a full-disk or read-only target on
desktop would also silently "succeed"), and it is what masked Issue B.

### Issue B — Android Storage Access Framework is not handled (root cause)

On Android, Qt's `FolderDialog` does **not** yield a filesystem path. It returns
a **Storage Access Framework (SAF) tree URI**, e.g.:

```
content://com.android.externalstorage.documents/tree/primary%3ADownload%2FTemp
```

The "Allow simsapadhammareader to access folder?" prompt is Android granting a
SAF permission on that URI — not a directory handle.

`qurl_to_local_path` (`bridges/src/sutta_bridge.rs:546`) only special-cases
Windows drive letters; for everything else it returns `url.path()` verbatim. For
the content URI above that is roughly `/tree/primary:Download/Temp`, a path that
does not exist on the filesystem, so `File::create()` in `save_to_file` fails
with `ENOENT`.

Even if the real underlying path (`/storage/emulated/0/Download/Temp`) were
resolved, a direct `std::fs` write would **still** fail: the app targets
`targetSdkVersion 35` (`android/build.gradle:90`), where **scoped storage**
forbids direct writes to shared storage. `android:requestLegacyExternalStorage="true"`
(`android/AndroidManifest.xml:21`) has **no effect** on Android 11+ (API 30+).
The only supported way to write into a SAF-granted tree is through Android's
`ContentResolver` / `DocumentsContract` API — from Rust that means JNI.

**Enabler already in place:** `ndk_context` is initialized at startup
(`backend/src/lib.rs:180` `init_android_context`, called from `gui.cpp` after
`QApplication`), so `ndk_context::android_context()` gives us the `JavaVM` and
the Activity `Context` needed to reach `ContentResolver` via JNI. The `jni`
crate is **not** yet a dependency; `ndk-context = "0.1"` already is
(`backend/Cargo.toml:60`).

`check_file_exists_in_folder` (`bridges/src/sutta_bridge.rs:2770`) has the same
content-URI blind spot — it joins the fake path and `std::fs`-stats it, so on
Android it always answers "does not exist," defeating the Gloss/Prompts
overwrite-confirmation prompts.

## Relevant Files

- `bridges/src/sutta_bridge.rs` — `save_file` (:2755, discards result + no SAF
  branch), `check_file_exists_in_folder` (:2770), `qurl_to_local_path` (:546,
  the desktop path-mapping helper). The CXX-Qt bridge decl for `save_file` is at
  :1076.
- `backend/src/lib.rs` — `save_to_file` (:1145), `init_android_context` (:180,
  proves `ndk_context` is live). New Android SAF writer module lands here or in
  a new `backend/src/android_saf.rs`.
- `backend/Cargo.toml` — has `ndk-context = "0.1"` (:60); needs `jni` added under
  an Android-only target block.
- `assets/qml/AboutDialog.qml` — the log "Save As…" `FolderDialog` (:274) and the
  only place that already logs `!ok` (:285). Success is *not* currently surfaced
  to the user, only failure.
- `assets/qml/GlossTab.qml` — `save_file` at :360 and the multi-file export loop
  at :1569; `check_file_exists_in_folder` at :404 and the folder listing used for
  the existing-files warning (:376–394).
- `assets/qml/PromptsTab.qml` — `save_file` at :597; `check_file_exists_in_folder`
  at :608.
- `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` — qmllint stub for
  `save_file` (:453) and `check_file_exists_in_folder`; update only if a
  signature changes.
- `android/AndroidManifest.xml` — the stale `requestLegacyExternalStorage`
  attribute (:21); no storage permissions are declared (SAF needs none).
- `docs/` — no doc covers file saving yet; a new one is warranted (see 4.0).
- `PROJECT_MAP.md` — record the new SAF writer entry point.

## Reuse, Adaptation & New Code

**Reuse as-is:**
- `ndk_context::android_context()` (already live — `backend/src/lib.rs:180`) gives the
  `JavaVM*` and Activity `Context` global ref. No new C++/JNI plumbing needed.
- **`jni` crate v0.22 is already compiled into the tree via cpal 0.18.1**
  (`bridges/Cargo.lock:596` → `jni 0.22.4`), our sibling Android audio dep that
  also uses `ndk_context`. Reuse that exact version — do **not** pull a different
  major (0.21 also appears transitively; adding a third copy is waste/risk).
- `QUrl::to_encoded()` (`cxx-qt-lib` qurl.rs:392 → `QUrl::toEncoded()`) returns
  the **fully percent-encoded** URL as `QByteArray` — the correct source for the
  SAF tree URI (see the trap below).
- Desktop path is unchanged: `qurl_to_local_path` + `std::fs`.
- The QML callers already branch on a `bool` return (`AboutDialog.qml:285`,
  Gloss/Prompts success dialogs) — once `save_file` returns a real bool, no QML
  signature changes are required. `check_file_exists_in_folder` likewise already
  returns `bool` to existing call sites.

**Adapt:**
- `save_file` (`sutta_bridge.rs:2755`) → dispatch on `folder_url.scheme()`; return
  the real write outcome.
- `check_file_exists_in_folder` (`sutta_bridge.rs:2770`) → SAF branch for
  `content://`.
- Drop the no-op `requestLegacyExternalStorage` (`AndroidManifest.xml:21`).

**New code:**
- `save_to_file_checked(data, path) -> Result<(), std::io::Error>` in
  `backend/src/lib.rs` (**additive** — see task 1.1; do not change the existing
  `save_to_file -> String`, it has 3 other callers).
- `backend/src/android_saf.rs` (`#[cfg(target_os = "android")]`): `write_to_tree_uri`,
  a shared `find_child_by_name` (used by both write-overwrite and existence
  check), and a MIME-from-extension helper. All JNI via the `jni` crate.
- `jni` dependency line in `backend/Cargo.toml` under an Android-only target block.

## Guard Against (consistency review)

- **Full encoded URI, not `.path()` — the #1 implementation trap.** The SAF
  writer must receive the entire `content://authority/tree/…` string, so it must
  use `folder_url.to_encoded()`. **Do not** reuse `qurl_to_local_path` for the
  SAF branch (it returns `.path()`, dropping scheme + authority), and **do not**
  use `to_qstring()`/`Display` (`toString()`/`toDisplayString()` pretty-**decode**
  `%3A`→`:` and `%2F`→`/`, corrupting the URI for Android's `Uri.parse`). Pass the
  `toEncoded` bytes → `&str` straight through to JNI.
- **Do not pre-strip the scheme in QML.** Callers pass the raw `selectedFolder`
  QUrl (good); the SAF branch needs scheme + authority. (Contrast the *import*
  dialog's `strip_file_scheme`, `DictionaryImportDialog.qml:88` — that's the read
  path, out of scope.)
- **JNI thread attachment.** The multi-file Anki save (`GlossTab.qml:1569`) runs
  in the QML signal handler `handle_anki_export_results` on the **GUI thread**
  (the heavy work is on a spawned thread in `export_anki_csv_background`, results
  delivered via signal). Regardless of caller thread, `write_to_tree_uri` must
  `attach_current_thread` and hold the `AttachGuard` for the duration of the JNI
  calls — never assume the current thread is attached.
- **Additive `save_to_file`.** Its 3 unrelated consumers — `lib.rs:450`
  (api-port file, ignores result), `storage_manager.rs:66` and `api.rs:1898`
  (both `info(&msg)`) — write app-private storage and must stay untouched.
- **Overwrite semantics parity.** SAF `write_to_tree_uri` must truncate/replace an
  existing same-name document (find via the shared `find_child_by_name`), or
  Android's `createDocument` yields `log (1).txt` duplicates — diverging from the
  desktop `File::create` truncate behavior the Gloss/Prompts overwrite prompts
  assume.

## Out of Scope (acknowledged, not overlooked)

- **Read-side SAF** (importing a StarDict *folder* via `DictionaryImportDialog`'s
  `FolderDialog` → `strip_file_scheme` + scan) hits the same `content://` wall on
  Android but for *reading*, and is a separate feature from the reported save bug.
  Track as a follow-up if folder-based dictionary import is needed on Android;
  the `android_saf` module from 2.0 is the natural home for a future
  `read_from_tree_uri`.

### Notes

- Staging rule: after each top-level task the app must compile (`make build -B`);
  run tests only after all sub-tasks of a top-level task are done. Docs-only
  sub-tasks need no build.
- The Android SAF code path (2.0) **cannot be exercised from a desktop `make
  build -B`** — it is `#[cfg(target_os = "android")]`. Desktop builds must keep
  compiling with the Android branch feature-gated out; the Android branch is
  verified by the user building/deploying the APK from Qt Creator and repeating
  the reporter's steps. Keep the JNI code isolated so a typo there can't break
  the desktop build.
- Do **not** attempt to reach shared storage with `std::fs` on Android or to
  re-add broad storage permissions — SAF is the supported route on targetSdk 35
  and needs no manifest permission.
- No new SQLite writes here, so no `ANALYZE` considerations.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, check it off in this markdown file by
changing `- [ ]` to `- [x]`. Update the file after completing each **sub-task**,
not only after a whole parent task.

## Tasks

### 1.0 Make `save_file` report real success/failure (Issue A)

**Specs.** `save_to_file` (`backend/src/lib.rs:1145`) already returns a `String`
that encodes the outcome, but its shape (a success/failure *message*) is awkward
to branch on. Give the write path a real boolean/`Result` outcome and have
`save_file` return it, so every QML caller's guard (`AboutDialog.qml:285`, the
Gloss/Prompts success dialogs) becomes truthful. This is prerequisite to
Issue B: without it, the Android SAF write would also fail silently and be
un-diagnosable. Log the failure reason in the backend (`error(...)`) so the Qt
Creator console shows it even when the QML layer only has a bool.

**Depends on:** nothing (do first — it makes 2.0 debuggable).

- [x] 1.0 Make `save_file` surface write failures instead of always returning `true` (Issue A)
  - [x] 1.1 In `backend/src/lib.rs`, **add** `save_to_file_checked(data, path) -> Result<(), std::io::Error>` (additive). Do **not** change the existing `save_to_file -> String` — it has 3 other consumers that must stay untouched: `lib.rs:450` (api-port file, ignores the result), `storage_manager.rs:66` and `api.rs:1898` (both `info(&msg)` the returned string), all writing app-private storage. Optionally reimplement `save_to_file` as a thin wrapper over the checked variant (format the message from the `Result`) to avoid duplicated `File::create` logic.
  - [x] 1.2 In `bridges/src/sutta_bridge.rs` `save_file` (:2755), use `save_to_file_checked` and return the actual write outcome (`false` on failure) instead of unconditional `true`; log the failure with the target path and reason via `error(...)` so the Qt Creator console shows it even though QML only receives a bool.
  - [x] 1.3 In `assets/qml/AboutDialog.qml`, optionally surface success to the user too (a small confirmation), so a working save is visibly distinct from a failure — mirror the "Exported as…" pattern used by Gloss/Prompts. (Failure already logs via :285.)
  - [x] 1.4 `make build -B` clean. (Desktop still writes via `std::fs`; on a normal target this is unchanged. No test run needed until end of parent task — this is a small correctness change.)

### 2.0 Write through the Android ContentResolver for `content://` URIs (Issue B)

**Specs.** When the folder URL is a `content://` SAF tree URI (Android),
`std::fs` cannot write it; route through `ContentResolver` via JNI instead. Use
`ndk_context::android_context()` (already initialized — `backend/src/lib.rs:180`)
to obtain the `JavaVM` and Activity `Context`, attach the current thread, and:
(1) build a `DocumentFile`/`DocumentsContract` child document with the given
filename + MIME under the granted tree (create, or truncate-if-exists to match
overwrite semantics), and (2) write the bytes to the resolver's `OutputStream`.
Keep the whole Android path behind `#[cfg(target_os = "android")]` in an isolated
module so a JNI mistake cannot break the desktop build. `save_file` becomes a
dispatch: if the URL scheme is `content` → SAF writer (Android); otherwise the
existing `qurl_to_local_path` + `std::fs` path (desktop, and any `file://` URL).
Add `jni` to `backend/Cargo.toml` under `[target.'cfg(target_os = "android")'.dependencies]`.

**Reasoning for JNI vs. alternatives.** There is no pure-Rust SAF API; the SAF is
a Java/Android-framework construct. `ndk_context` already exposes the VM +
Context (set up for cpal), so no new C++ plumbing is required — this is the
lowest-surface option. Broad storage permissions + `std::fs` are not viable on
targetSdk 35 (see Issue B).

**Depends on:** 1.0 (so a SAF failure is actually reported and loggable).

- [x] 2.0 Route Android `content://` saves through the ContentResolver via JNI (Issue B)
  - [x] 2.1 Add `jni` to `backend/Cargo.toml` under `[target.'cfg(target_os = "android")'.dependencies]` so desktop builds don't pull it. **Deviation from the original plan (pin 0.22):** cpal 0.18.1's `jni 0.22.4` turned out to be an experimental redesign (closure-scoped `Env`, typed method names/signatures) that is high-risk to write blind against with no local Android toolchain. `app_dirs2` **already** compiles `jni 0.21.1` for the Android target, so pinning `jni = "0.21"` reuses that existing copy (no third copy — the tree still has exactly 0.21.1 + cpal's 0.22.4) and gives the stable classic API. Verified via `cargo tree --target aarch64-linux-android`.
  - [x] 2.2 Create `backend/src/android_saf.rs` (module gated `#[cfg(target_os = "android")]`): `write_to_tree_uri(tree_uri, filename, mime, data) -> Result<(), String>` gets the VM+Context from `ndk_context::android_context()`, `attach_current_thread` (AttachGuard held for the whole body), finds-or-creates the child document, opens the resolver `OutputStream`, writes/flushes/closes. **Overwrite parity** via shared `find_child_doc_uri` (used by 2.4 through `child_exists`) — truncates an existing same-name doc (`"wt"`) instead of `createDocument` (which yields `file (1).txt`). Descriptive `String` errors feed 1.0's logging. Type-checked standalone against jni 0.21.1.
  - [x] 2.3 In `bridges/src/sutta_bridge.rs` `save_file`: `#[cfg(target_os = "android")]` branch on `folder_url.scheme() == "content"` → SAF writer, passing `folder_url.to_encoded()` (fully-encoded, not `.path()`/`toString()`). MIME via `android_saf::mime_from_filename` covering `.txt`/`.html`/`.md`/`.org`/`.csv` with `application/octet-stream` fallback. Otherwise the existing `std::fs` path; real bool from 1.0 preserved.
  - [x] 2.4 Fix `check_file_exists_in_folder` (:2770) for SAF: Android `content://` folder → `android_saf::child_exists` (reuses `find_child_doc_uri`), passing `folder_url.to_encoded()`; otherwise the existing `std::fs` check. On SAF query failure it logs and returns `false` (does not block the save; the write path handles overwrite) — documented in the branch.
  - [x] 2.5 Removed `android:requestLegacyExternalStorage="true"` from `android/AndroidManifest.xml`. Confirmed nothing references legacy external storage / `WRITE_EXTERNAL_STORAGE` / `MANAGE_EXTERNAL_STORAGE`.
  - [x] 2.6 `make build -B` clean on desktop (Android branch cfg'd out); backend `cargo test` all pass. The Android branch's Rust was type-checked standalone against jni 0.21.1 (the C sys-crates need the NDK toolchain, unavailable locally); full APK build + on-device behavior verified by the user in 3.0.

### 3.0 On-device verification (Issue A + B)

**Specs.** The SAF path cannot run under desktop tests; it must be exercised on a
real Android device by repeating the reporter's exact steps, plus the other
`save_file` callers. This is a user-run checklist (agents avoid GUI runs, and the
APK is built from Qt Creator).

**Depends on:** 1.0, 2.0.

- [x] 3.0 Verify on Android device (user-run checklist)
  - [x] 3.1 AboutDialog → Save As… `log.txt` → Download/Temp → Allow: file **is** written, and re-opening the folder in a file manager shows `log.txt` with the log contents. Test a **nested/newly-created** folder (the reporter's exact case, whose tree URI carries encoded `%3A`/`%2F`) to prove the `to_encoded` round-trip through `Uri.parse` holds. A forced failure (e.g. denied permission) now shows an error, not silent success.
  - [x] 3.2 Gloss export and Prompts export to a chosen SAF folder write the file; the overwrite-confirmation prompt correctly detects an existing file on a second export.
  - [x] 3.3 Gloss-tab multi-file export (`GlossTab.qml:1569`) writes all files.
  - [x] 3.4 Regression: desktop (Linux) saves for all of the above still work unchanged.

### 4.0 Documentation and housekeeping

**Specs.** No existing doc covers file saving; the SAF/JNI mechanism and the
"content:// vs file path" dispatch are non-obvious and load-bearing (mirrors the
`docs/pure-rust-audio-backend.md` ndk_context note). Record it and update the
map.

**Depends on:** 1.0–2.0 (documents their outcomes).

- [x] 4.0 Document the save path and update the map
  - [x] 4.1 Add `docs/android-file-saving-saf.md`: why `FolderDialog` returns a `content://` tree URI on Android, the `save_file` scheme dispatch, the JNI ContentResolver writer + its reliance on the already-initialized `ndk_context` (cross-link `pure-rust-audio-backend.md`), the create/overwrite semantics, why `std::fs`/legacy external storage don't work on targetSdk 35, and the `check_file_exists_in_folder` SAF behavior. Note the Issue-A silent-success bug as the reason failures were invisible.
  - [x] 4.2 Add a one-line pointer to the new doc in `CLAUDE.md`/`AGENTS.md`'s notable-feature-docs list (edit `AGENTS.md`, the real file — `CLAUDE.md` is a symlink), and update `PROJECT_MAP.md` with the new `android_saf` module and the `save_file` dispatch.
  - [x] 4.3 Final pass: last `make build -B` clean + backend tests pass (only docs `.md` changed since); the 3.0 checklist was handed to the user and confirmed passing on-device; checkboxes ticked.
