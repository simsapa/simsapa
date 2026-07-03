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
//! See `docs/android-file-saving-saf.md` and `docs/pure-rust-audio-backend.md`.

use jni::objects::{JByteArray, JObject, JString, JValue};
use jni::JavaVM;

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
    } else {
        "application/octet-stream"
    }
}

/// Attach the current thread and obtain (VM guard, ContentResolver, parsed tree
/// URI, tree document id). The returned `AttachGuard` must be kept alive for the
/// duration of any further JNI calls, so this returns it to the caller.
///
/// Returns `Err(String)` with a descriptive message on any JNI failure (these
/// feed the caller's `error(...)` logging).
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
