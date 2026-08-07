//! Android Storage Access Framework (SAF) writer.
//!
//! On Android, Qt's `FolderDialog` does not return a filesystem path — it returns
//! a SAF *tree* URI such as
//! `content://com.android.externalstorage.documents/tree/primary%3ADownload%2FTemp`,
//! and `targetSdkVersion 35` scoped storage forbids `std::fs` writes into shared
//! storage. The only supported way to write into a SAF-granted tree is through
//! Android's `ContentResolver` / `DocumentsContract`, reached here via JNI using
//! the `JavaVM` + Activity `Context` that `init_android_context` (in `lib.rs`)
//! already registered with `ndk_context` for the audio backend.
//!
//! This file also carries the **read** side: `probe_document_uri` opens a
//! provider-backed document through `ContentResolver.openInputStream`. The PRD
//! words that as a fix to `copy_content_uri_to_temp_file` in `cpp/utils.cpp`; it
//! is implemented in Rust here instead, because this file already holds the whole
//! JNI stack the write path needs — the same `jni 0.21` pin, the same
//! `ndk_context`, the same error-string discipline — and the PRD itself frames
//! the work as "the read-side mirror" of `save_file`. That is a placement
//! decision, not a scope change; it is recorded here so it is not re-litigated.
//!
//! Reading must go through `openInputStream` rather than `QFile`. `QFile` reaches
//! a document only via Qt's `QAndroidContentFileEngine`, which handles the
//! `content://` scheme alone — and a non-`content://` provider scheme is exactly
//! what the diagnostic exists to detect.
//!
//! See `docs/android-file-saving-saf.md`, `docs/file-selection-test.md` and
//! `docs/pure-rust-audio-backend.md`.

use std::time::Instant;

use jni::objects::{JByteArray, JObject, JString, JValue};
use jni::JavaVM;

use crate::picker_url::DocumentProbe;

/// Map a filename extension to a MIME type for `DocumentsContract.createDocument`.
/// Covers every extension actually written by the `save_file` callers, with a
/// safe fallback so a new export type can never break the write.
pub fn mime_from_filename(filename: &str) -> &'static str {
    let lower = filename.to_lowercase();
    if lower.ends_with(".txt") {
        "text/plain"
    } else if lower.ends_with(".html") || lower.ends_with(".htm") {
        "text/html"
    } else if lower.ends_with(".md") {
        "text/markdown"
    } else if lower.ends_with(".org") {
        "text/plain"
    } else if lower.ends_with(".csv") {
        "text/csv"
    } else if lower.ends_with(".json") {
        "application/json"
    } else if lower.ends_with(".docx") {
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
    } else {
        "application/octet-stream"
    }
}

/// Attach the current thread and obtain (VM guard, ContentResolver).
///
/// The tree-agnostic half of the old `attach()`: everything a *document* URI can
/// use. `attach()` itself additionally resolves a tree document id, which a plain
/// document URI has no answer for, so the two had to be separated before the
/// read path could reuse any of this.
///
/// The returned `AttachGuard` must be kept alive for the duration of any further
/// JNI calls, so this returns it to the caller.
///
/// Returns `Err(String)` with a descriptive message on any JNI failure (these
/// feed the caller's `error(...)` logging).
fn attach_resolver<'a>(
    vm: &'a JavaVM,
) -> Result<(jni::AttachGuard<'a>, JObject<'a>), String> {
    let mut env = vm
        .attach_current_thread()
        .map_err(|e| format!("attach_current_thread: {e}"))?;

    let ctx = ndk_context::android_context();
    let context = unsafe { JObject::from_raw(ctx.context() as jni::sys::jobject) };

    let resolver = env
        .call_method(
            &context,
            "getContentResolver",
            "()Landroid/content/ContentResolver;",
            &[],
        )
        .map_err(|e| format!("getContentResolver: {e}"))?
        .l()
        .map_err(|e| format!("getContentResolver .l(): {e}"))?;

    Ok((env, resolver))
}

/// Attach the current thread and obtain (VM guard, ContentResolver, parsed tree
/// URI, tree document id), for the SAF **tree** write path.
///
/// A thin wrapper over `attach_resolver` plus the tree-specific `Uri.parse` +
/// `getTreeDocumentId`. Behaviour is unchanged from before the split.
fn attach<'a>(
    vm: &'a JavaVM,
    tree_uri: &str,
) -> Result<
    (
        jni::AttachGuard<'a>,
        JObject<'a>,
        JObject<'a>,
        JObject<'a>,
    ),
    String,
> {
    let (mut env, resolver) = attach_resolver(vm)?;

    let uri_str = env
        .new_string(tree_uri)
        .map_err(|e| format!("new_string(tree_uri): {e}"))?;
    let tree_uri_obj = env
        .call_static_method(
            "android/net/Uri",
            "parse",
            "(Ljava/lang/String;)Landroid/net/Uri;",
            &[JValue::Object(&uri_str)],
        )
        .map_err(|e| format!("Uri.parse: {e}"))?
        .l()
        .map_err(|e| format!("Uri.parse .l(): {e}"))?;

    let tree_doc_id = env
        .call_static_method(
            "android/provider/DocumentsContract",
            "getTreeDocumentId",
            "(Landroid/net/Uri;)Ljava/lang/String;",
            &[JValue::Object(&tree_uri_obj)],
        )
        .map_err(|e| format!("getTreeDocumentId: {e}"))?
        .l()
        .map_err(|e| format!("getTreeDocumentId .l(): {e}"))?;

    Ok((env, resolver, tree_uri_obj, tree_doc_id))
}

/// Find a child document with display name `name` directly under the tree.
/// Returns the child's document `Uri` object if found, else `None`. Shared by
/// the write (overwrite parity) and existence-check paths.
fn find_child_doc_uri<'a>(
    env: &mut jni::JNIEnv<'a>,
    resolver: &JObject,
    tree_uri_obj: &JObject,
    tree_doc_id: &JObject,
    name: &str,
) -> Result<Option<JObject<'a>>, String> {
    let children_uri = env
        .call_static_method(
            "android/provider/DocumentsContract",
            "buildChildDocumentsUriUsingTree",
            "(Landroid/net/Uri;Ljava/lang/String;)Landroid/net/Uri;",
            &[JValue::Object(tree_uri_obj), JValue::Object(tree_doc_id)],
        )
        .map_err(|e| format!("buildChildDocumentsUriUsingTree: {e}"))?
        .l()
        .map_err(|e| format!("buildChildDocumentsUriUsingTree .l(): {e}"))?;

    // projection = { "document_id", "_display_name" }
    let projection = env
        .new_object_array(2, "java/lang/String", JObject::null())
        .map_err(|e| format!("new_object_array: {e}"))?;
    let col_id = env
        .new_string("document_id")
        .map_err(|e| format!("new_string(document_id): {e}"))?;
    let col_name = env
        .new_string("_display_name")
        .map_err(|e| format!("new_string(_display_name): {e}"))?;
    env.set_object_array_element(&projection, 0, &col_id)
        .map_err(|e| format!("set_object_array_element 0: {e}"))?;
    env.set_object_array_element(&projection, 1, &col_name)
        .map_err(|e| format!("set_object_array_element 1: {e}"))?;

    let null = JObject::null();
    let cursor = env
        .call_method(
            resolver,
            "query",
            "(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;",
            &[
                JValue::Object(&children_uri),
                JValue::Object(&projection),
                JValue::Object(&null),
                JValue::Object(&null),
                JValue::Object(&null),
            ],
        )
        .map_err(|e| format!("resolver.query: {e}"))?
        .l()
        .map_err(|e| format!("resolver.query .l(): {e}"))?;

    if cursor.is_null() {
        return Ok(None);
    }

    let mut found: Option<JObject> = None;
    loop {
        let has_next = env
            .call_method(&cursor, "moveToNext", "()Z", &[])
            .map_err(|e| format!("cursor.moveToNext: {e}"))?
            .z()
            .map_err(|e| format!("cursor.moveToNext .z(): {e}"))?;
        if !has_next {
            break;
        }

        let disp = env
            .call_method(&cursor, "getString", "(I)Ljava/lang/String;", &[JValue::Int(1)])
            .map_err(|e| format!("cursor.getString(display_name): {e}"))?
            .l()
            .map_err(|e| format!("cursor.getString(display_name) .l(): {e}"))?;
        let disp_str: String = env
            .get_string(&JString::from(disp))
            .map_err(|e| format!("get_string(display_name): {e}"))?
            .into();

        if disp_str == name {
            let doc_id = env
                .call_method(&cursor, "getString", "(I)Ljava/lang/String;", &[JValue::Int(0)])
                .map_err(|e| format!("cursor.getString(document_id): {e}"))?
                .l()
                .map_err(|e| format!("cursor.getString(document_id) .l(): {e}"))?;
            let doc_uri = env
                .call_static_method(
                    "android/provider/DocumentsContract",
                    "buildDocumentUriUsingTree",
                    "(Landroid/net/Uri;Ljava/lang/String;)Landroid/net/Uri;",
                    &[JValue::Object(tree_uri_obj), JValue::Object(&doc_id)],
                )
                .map_err(|e| format!("buildDocumentUriUsingTree: {e}"))?
                .l()
                .map_err(|e| format!("buildDocumentUriUsingTree .l(): {e}"))?;
            found = Some(doc_uri);
            break;
        }
    }

    env.call_method(&cursor, "close", "()V", &[])
        .map_err(|e| format!("cursor.close: {e}"))?;

    Ok(found)
}

/// Ask the provider for `OpenableColumns.DISPLAY_NAME` and `SIZE` in one cursor
/// query.
///
/// Metadata is a courtesy, not a contract: a provider may return a null cursor,
/// an empty cursor, or omit either column. None of that is a reason to fail the
/// probe — whether the document *reads* is the question — so every shortfall
/// becomes a note and the caller carries on.
fn query_openable_columns(
    env: &mut jni::JNIEnv,
    resolver: &JObject,
    uri_obj: &JObject,
) -> Result<(Option<String>, Option<i64>, Vec<String>), String> {
    let mut notes: Vec<String> = Vec::new();

    let projection = env
        .new_object_array(2, "java/lang/String", JObject::null())
        .map_err(|e| format!("new_object_array: {e}"))?;
    let col_name = env
        .new_string("_display_name")
        .map_err(|e| format!("new_string(_display_name): {e}"))?;
    let col_size = env
        .new_string("_size")
        .map_err(|e| format!("new_string(_size): {e}"))?;
    env.set_object_array_element(&projection, 0, &col_name)
        .map_err(|e| format!("set_object_array_element 0: {e}"))?;
    env.set_object_array_element(&projection, 1, &col_size)
        .map_err(|e| format!("set_object_array_element 1: {e}"))?;

    let null = JObject::null();
    let cursor = match env.call_method(
        resolver,
        "query",
        "(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;",
        &[
            JValue::Object(uri_obj),
            JValue::Object(&projection),
            JValue::Object(&null),
            JValue::Object(&null),
            JValue::Object(&null),
        ],
    ) {
        Ok(v) => v.l().map_err(|e| format!("resolver.query .l(): {e}"))?,
        Err(e) => {
            // A thrown Java exception stays pending and would poison every
            // later JNI call on this thread, so clear it before continuing.
            let _ = env.exception_clear();
            notes.push(format!("metadata query threw: {e}"));
            return Ok((None, None, notes));
        }
    };

    if cursor.is_null() {
        notes.push("metadata query returned a null cursor".to_string());
        return Ok((None, None, notes));
    }

    // Not `?`: an early return here would skip the `cursor.close()` below, and
    // this is the one step in the function that could take that path. Every
    // other provider quirk becomes a note and the query carries on.
    let has_row = match env
        .call_method(&cursor, "moveToFirst", "()Z", &[])
        .map_err(|e| format!("cursor.moveToFirst: {e}"))
        .and_then(|v| v.z().map_err(|e| format!("cursor.moveToFirst .z(): {e}")))
    {
        Ok(has_row) => has_row,
        Err(e) => {
            let _ = env.exception_clear();
            notes.push(format!("metadata cursor unusable: {e}"));
            let _ = env.call_method(&cursor, "close", "()V", &[]);
            return Ok((None, None, notes));
        }
    };

    let mut display_name = None;
    let mut size = None;

    if has_row {
        match env.call_method(&cursor, "getString", "(I)Ljava/lang/String;", &[JValue::Int(0)]) {
            Ok(v) => match v.l() {
                Ok(obj) if !obj.is_null() => match env.get_string(&JString::from(obj)) {
                    Ok(s) => display_name = Some(s.into()),
                    Err(e) => notes.push(format!("display_name decode failed: {e}")),
                },
                Ok(_) => notes.push("provider reported a null display name".to_string()),
                Err(e) => notes.push(format!("display_name .l() failed: {e}")),
            },
            Err(e) => {
                let _ = env.exception_clear();
                notes.push(format!("provider has no _display_name column: {e}"));
            }
        }

        let size_is_null = env
            .call_method(&cursor, "isNull", "(I)Z", &[JValue::Int(1)])
            .ok()
            .and_then(|v| v.z().ok())
            .unwrap_or(true);

        if size_is_null {
            notes.push("provider reported no size".to_string());
        } else {
            match env.call_method(&cursor, "getLong", "(I)J", &[JValue::Int(1)]) {
                Ok(v) => match v.j() {
                    Ok(n) => size = Some(n),
                    Err(e) => notes.push(format!("size .j() failed: {e}")),
                },
                Err(e) => {
                    let _ = env.exception_clear();
                    notes.push(format!("provider has no _size column: {e}"));
                }
            }
        }
    } else {
        notes.push("metadata query returned no rows".to_string());
    }

    let _ = env.call_method(&cursor, "close", "()V", &[]);

    Ok((display_name, size, notes))
}

/// Open a provider-backed document and read a capped prefix of it, reporting
/// what happened at each step.
///
/// `uri` must be the **fully-encoded** form (see the module docs). Nothing is
/// written anywhere: the bytes read are counted and discarded, so unlike the
/// staging path there is no file to clean up and therefore no `Drop` guard here.
/// Do not add one for a file that does not exist.
pub fn probe_document_uri(uri: &str, cap_bytes: usize) -> DocumentProbe {
    let vm = match unsafe {
        JavaVM::from_raw(ndk_context::android_context().vm() as *mut jni::sys::JavaVM)
    } {
        Ok(vm) => vm,
        Err(e) => return DocumentProbe::failed("JavaVM::from_raw", e),
    };

    let (mut env, resolver) = match attach_resolver(&vm) {
        Ok(pair) => pair,
        Err(e) => return DocumentProbe::failed("attach resolver", e),
    };

    let uri_j = match env.new_string(uri) {
        Ok(s) => s,
        Err(e) => return DocumentProbe::failed("new_string(uri)", e),
    };
    let uri_obj = match env.call_static_method(
        "android/net/Uri",
        "parse",
        "(Ljava/lang/String;)Landroid/net/Uri;",
        &[JValue::Object(&uri_j)],
    ) {
        Ok(v) => match v.l() {
            Ok(o) => o,
            Err(e) => return DocumentProbe::failed("Uri.parse .l()", e),
        },
        Err(e) => {
            let _ = env.exception_clear();
            return DocumentProbe::failed("URI parse", e);
        }
    };

    let mut probe = DocumentProbe::default();

    // Metadata first, and non-fatally: if the document turns out to be
    // unreadable, knowing whether the provider could even name it is part of
    // the finding.
    match query_openable_columns(&mut env, &resolver, &uri_obj) {
        Ok((name, size, notes)) => {
            probe.display_name = name;
            probe.size = size;
            probe.notes.extend(notes);
        }
        Err(e) => probe.notes.push(format!("metadata query failed: {e}")),
    }

    let open_started = Instant::now();
    let stream = match env.call_method(
        &resolver,
        "openInputStream",
        "(Landroid/net/Uri;)Ljava/io/InputStream;",
        &[JValue::Object(&uri_obj)],
    ) {
        Ok(v) => {
            probe.open_ms = Some(open_started.elapsed().as_millis());
            match v.l() {
                Ok(o) => o,
                Err(e) => {
                    probe.error = Some(format!("resolver open .l(): {e}"));
                    return probe;
                }
            }
        }
        Err(e) => {
            probe.open_ms = Some(open_started.elapsed().as_millis());
            let _ = env.exception_clear();
            probe.error = Some(format!("resolver open: {e}"));
            return probe;
        }
    };

    if stream.is_null() {
        probe.error = Some("resolver open: openInputStream returned null".to_string());
        return probe;
    }

    probe.opened = true;

    // Read in fixed-size chunks up to the cap, discarding the bytes. The last
    // chunk is shortened so the cap is exact rather than approximate.
    const CHUNK: usize = 64 * 1024;
    let buf: JByteArray = match env.new_byte_array(CHUNK as i32) {
        Ok(b) => b,
        Err(e) => {
            probe.error = Some(format!("read buffer: {e}"));
            close_stream(&mut env, &stream);
            return probe;
        }
    };

    let read_started = Instant::now();
    let mut total: u64 = 0;

    loop {
        let remaining = cap_bytes.saturating_sub(total as usize);
        if remaining == 0 {
            probe.reached_cap = true;
            break;
        }
        let want = remaining.min(CHUNK) as i32;

        let n = match env.call_method(
            &stream,
            "read",
            "([BII)I",
            &[JValue::Object(&buf), JValue::Int(0), JValue::Int(want)],
        ) {
            Ok(v) => match v.i() {
                Ok(n) => n,
                Err(e) => {
                    probe.error = Some(format!("read .i(): {e}"));
                    break;
                }
            },
            Err(e) => {
                let _ = env.exception_clear();
                probe.error = Some(format!("read: {e}"));
                break;
            }
        };

        // -1 is end of stream: the document was smaller than the cap and has
        // been read in full.
        //
        // 0 is treated as end of stream too, and that is deliberate. With a
        // positive length `InputStream.read` is not allowed to return 0, but
        // this probe exists to exercise *unusual* providers on a device we
        // cannot attach a debugger to: a provider that returned 0 would leave
        // `total` unchanged, so the loop would never make progress and the test
        // would hang on a worker thread holding the keep-screen-on lock. A
        // short read reported honestly beats a frozen app.
        if n <= 0 {
            if n == 0 {
                probe
                    .notes
                    .push("provider returned a 0-byte read; treated as end of stream".to_string());
            }
            break;
        }
        total += n as u64;
    }

    probe.read_ms = Some(read_started.elapsed().as_millis());
    probe.bytes_read = Some(total);

    // Close on every path, success and failure alike.
    close_stream(&mut env, &stream);

    probe
}

/// Close an `InputStream`, swallowing any failure. Called from both the success
/// and the error paths, where a close error is not the finding.
fn close_stream(env: &mut jni::JNIEnv, stream: &JObject) {
    if env.call_method(stream, "close", "()V", &[]).is_err() {
        let _ = env.exception_clear();
    }
}

/// Public existence check for `check_file_exists_in_folder`'s SAF branch.
pub fn child_exists(tree_uri: &str, filename: &str) -> Result<bool, String> {
    let vm = unsafe {
        JavaVM::from_raw(ndk_context::android_context().vm() as *mut jni::sys::JavaVM)
    }
    .map_err(|e| format!("JavaVM::from_raw: {e}"))?;

    let (mut env, resolver, tree_uri_obj, tree_doc_id) = attach(&vm, tree_uri)?;
    let found = find_child_doc_uri(&mut env, &resolver, &tree_uri_obj, &tree_doc_id, filename)?;
    Ok(found.is_some())
}

/// Write `data` as `filename` (`mime`) into the SAF-granted `tree_uri`. Truncates
/// and replaces an existing same-name document (overwrite parity with the desktop
/// `File::create` path), otherwise creates a new document.
pub fn write_to_tree_uri(
    tree_uri: &str,
    filename: &str,
    mime: &str,
    data: &[u8],
) -> Result<(), String> {
    let vm = unsafe {
        JavaVM::from_raw(ndk_context::android_context().vm() as *mut jni::sys::JavaVM)
    }
    .map_err(|e| format!("JavaVM::from_raw: {e}"))?;

    let (mut env, resolver, tree_uri_obj, tree_doc_id) = attach(&vm, tree_uri)?;

    // Overwrite parity: reuse an existing same-name document (truncate) instead
    // of createDocument, which would yield "file (1).txt" duplicates.
    let existing =
        find_child_doc_uri(&mut env, &resolver, &tree_uri_obj, &tree_doc_id, filename)?;

    let (target_uri, mode) = match existing {
        Some(uri) => (uri, "wt"),
        None => {
            let parent_doc_uri = env
                .call_static_method(
                    "android/provider/DocumentsContract",
                    "buildDocumentUriUsingTree",
                    "(Landroid/net/Uri;Ljava/lang/String;)Landroid/net/Uri;",
                    &[JValue::Object(&tree_uri_obj), JValue::Object(&tree_doc_id)],
                )
                .map_err(|e| format!("buildDocumentUriUsingTree(parent): {e}"))?
                .l()
                .map_err(|e| format!("buildDocumentUriUsingTree(parent) .l(): {e}"))?;

            let mime_j = env
                .new_string(mime)
                .map_err(|e| format!("new_string(mime): {e}"))?;
            let name_j = env
                .new_string(filename)
                .map_err(|e| format!("new_string(filename): {e}"))?;

            let new_uri = env
                .call_static_method(
                    "android/provider/DocumentsContract",
                    "createDocument",
                    "(Landroid/content/ContentResolver;Landroid/net/Uri;Ljava/lang/String;Ljava/lang/String;)Landroid/net/Uri;",
                    &[
                        JValue::Object(&resolver),
                        JValue::Object(&parent_doc_uri),
                        JValue::Object(&mime_j),
                        JValue::Object(&name_j),
                    ],
                )
                .map_err(|e| format!("createDocument: {e}"))?
                .l()
                .map_err(|e| format!("createDocument .l(): {e}"))?;

            if new_uri.is_null() {
                return Err(format!("createDocument returned null for {filename}"));
            }
            (new_uri, "w")
        }
    };

    let mode_j = env
        .new_string(mode)
        .map_err(|e| format!("new_string(mode): {e}"))?;
    let out = env
        .call_method(
            &resolver,
            "openOutputStream",
            "(Landroid/net/Uri;Ljava/lang/String;)Ljava/io/OutputStream;",
            &[JValue::Object(&target_uri), JValue::Object(&mode_j)],
        )
        .map_err(|e| format!("openOutputStream: {e}"))?
        .l()
        .map_err(|e| format!("openOutputStream .l(): {e}"))?;

    if out.is_null() {
        return Err(format!("openOutputStream returned null for {filename}"));
    }

    let jbytes: JByteArray = env
        .byte_array_from_slice(data)
        .map_err(|e| format!("byte_array_from_slice: {e}"))?;
    env.call_method(&out, "write", "([B)V", &[JValue::Object(&jbytes)])
        .map_err(|e| format!("OutputStream.write: {e}"))?;
    env.call_method(&out, "flush", "()V", &[])
        .map_err(|e| format!("OutputStream.flush: {e}"))?;
    env.call_method(&out, "close", "()V", &[])
        .map_err(|e| format!("OutputStream.close: {e}"))?;

    Ok(())
}
