# Android file saving via the Storage Access Framework (SAF)

How `SuttaBridge.save_file(...)` writes user-chosen files, and why Android needs
a completely different path from the desktop `std::fs` write.

Entry points:
- `bridges/src/sutta_bridge.rs` — `save_file`, `check_file_exists_in_folder`
  (the scheme dispatch), `qurl_to_local_path` (desktop path mapping).
- `backend/src/lib.rs` — `save_to_file_checked` (the checked desktop writer).
- `backend/src/android_saf.rs` — the JNI ContentResolver writer
  (`#[cfg(target_os = "android")]`).

## The two write paths

`save_file(folder_url, filename, content)` dispatches on the URL scheme:

- **Desktop / any `file://` URL** → `qurl_to_local_path(folder_url)` + join the
  filename + `save_to_file_checked` (`std::fs::File::create` + `write_all`).
  Unchanged, cross-platform.
- **Android `content://` URL** → `android_saf::write_to_tree_uri` via JNI (see
  below). Guarded by `#[cfg(target_os = "android")]`; on other platforms a
  `content://` URL (which should never occur) falls through to the `std::fs`
  path.

`check_file_exists_in_folder` dispatches the same way: `content://` →
`android_saf::child_exists`, otherwise the `std::fs` stat. This is what makes the
Gloss/Prompts overwrite-confirmation prompts work on Android.

## Why Android can't use `std::fs`

On Android, Qt's `FolderDialog` does **not** return a filesystem path. It returns
a **Storage Access Framework tree URI**, e.g.

```
content://com.android.externalstorage.documents/tree/primary%3ADownload%2FTemp
```

The "Allow … to access folder?" prompt is Android granting a SAF permission on
that URI — not a directory handle. Two independent walls block a direct write:

1. **The URI is not a path.** `QUrl::path()` on the URI above yields something
   like `/tree/primary:Download/Temp`, which does not exist on the filesystem —
   `File::create` fails with `ENOENT`. (This is why the old code, which reused
   `qurl_to_local_path`, silently failed: it built a bogus path.)
2. **Scoped storage.** The app targets `targetSdkVersion 35`, where scoped
   storage forbids direct `std::fs` writes into shared storage even if the real
   path (`/storage/emulated/0/Download/Temp`) were resolved.
   `android:requestLegacyExternalStorage="true"` has **no effect** on Android 11+
   (API 30+) and was removed from `android/AndroidManifest.xml`. SAF requires
   **no** manifest storage permission.

The only supported way to write into a SAF-granted tree is through Android's
`ContentResolver` / `DocumentsContract` — from Rust that means JNI.

## The JNI writer (`backend/src/android_saf.rs`)

Reuses the `JavaVM` + Activity `Context` that `init_android_context`
(`backend/src/lib.rs`, called from `cpp/gui.cpp` after `QApplication`) already
registered with `ndk_context` for the audio backend — so **no new C++ plumbing**
is needed. See [pure-rust-audio-backend.md](./pure-rust-audio-backend.md) for the
`ndk_context` setup.

`write_to_tree_uri(tree_uri, filename, mime, data)`:

1. `JavaVM::from_raw(ndk_context::android_context().vm())`, then
   `attach_current_thread` — the `AttachGuard` is **held for the whole body**
   (never assume the calling thread is attached; the Gloss multi-file Anki export
   runs the write from a QML signal handler).
2. `Uri.parse(tree_uri)` → `DocumentsContract.getTreeDocumentId` →
   `buildChildDocumentsUriUsingTree` / `buildDocumentUriUsingTree`.
3. **Overwrite parity:** `find_child_doc_uri` queries the tree's children
   (`_display_name`) for an existing document with the same name. If found, it is
   reused and opened `"wt"` (write-truncate); otherwise
   `DocumentsContract.createDocument` makes a new one, opened `"w"`. This matches
   the desktop `File::create` truncate behaviour — blindly calling
   `createDocument` would yield `log (1).txt` duplicates.
4. `ContentResolver.openOutputStream(uri, mode)` → `write([B)` → `flush` →
   `close`.

`child_exists(tree_uri, filename)` shares `find_child_doc_uri` and just reports
whether a match was found. On a JNI query failure it logs and returns `false`
(reports "not present" rather than blocking the save; the write path handles
overwrite either way).

`mime_from_filename` maps the extension for `createDocument`: `.txt`/`.org` →
`text/plain`, `.html` → `text/html`, `.md` → `text/markdown`, `.csv` →
`text/csv`, with `application/octet-stream` as the fallback so a new export type
can never break the write.

### The `to_encoded()` trap

The SAF writer must receive the **entire** `content://authority/tree/…` string,
so `save_file` passes `folder_url.to_encoded()` (fully percent-encoded). Do **not**:

- reuse `qurl_to_local_path` / `QUrl::path()` — it drops the scheme + authority;
- use `toString()` / `toDisplayString()` — they pretty-**decode** `%3A`→`:` and
  `%2F`→`/`, corrupting the URI so Android's `Uri.parse` reads the wrong tree.

The reporter's exact case (a newly-created nested folder like `Download/Temp`,
whose tree URI carries encoded `%3A`/`%2F`) is the regression guard for this
round-trip.

## The `jni` crate version

`backend/Cargo.toml` pins `jni = "0.21"` under
`[target.'cfg(target_os = "android")'.dependencies]`. `app_dirs2` already
compiles `jni 0.21.1` for Android, so this reuses that copy and adds none. (cpal
0.18.1 pulls `jni 0.22.4`, but 0.22.x is an experimental redesign with a
closure-scoped `Env` and typed method signatures; 0.21 is the stable classic
`JNIEnv` API this module uses.) The Rust in `android_saf.rs` is `cfg`-gated so a
JNI mistake cannot break the desktop build; it is verified by building the APK in
Qt Creator (the Android C toolchain / NDK is not part of the desktop `make build`).

## Issue A — why failures used to be invisible

Independently of Android, `save_file` used to call the backend writer, **discard
its result, and return `true` unconditionally**. Every QML guard
(`AboutDialog.qml`, the Gloss/Prompts "Exported as…" dialogs) therefore never saw
a failure — the Android write failed but the whole stack reported success. The
writer now returns a real `Result` (`save_to_file_checked`), `save_file` returns
the actual bool and logs the reason via `error(...)`, and `AboutDialog` shows a
"Saved: …" / "Failed to save…" dialog. This made the SAF work diagnosable.
