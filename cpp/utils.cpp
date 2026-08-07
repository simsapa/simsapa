#include <QDir>
#include <QFile>
#include <QString>
#include <QSysInfo>
#include <QStandardPaths>
#include <QStorageInfo>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonDocument>
#include <QGuiApplication>

#ifdef Q_OS_ANDROID
#include <QJniObject>
#include <QJniEnvironment>
#endif

#include "utils.h"

// The app's own logger (Rust side). On Android, Qt's qInfo()/qWarning() are
// tagged with the *application name*, not "Qt", so storage diagnostics logged
// that way do not appear alongside everything else under the `simsapa` logcat
// tag — which is where anyone debugging this feature will be looking.
extern "C" void log_info_c(const char* msg);
extern "C" void log_error_c(const char* msg);

QString get_internal_storage_path() {
    QString path = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation);
    return path;
}

QString get_app_assets_path() {
    QString path = get_internal_storage_path() + "/app-assets";
    return path;
}

// Informational only: this reports the Android status bar height, which is NOT
// the safe area (it excludes the display cutout and the navigation bar, and it
// is not per-window). Layout insets come from Qt, which binds ApplicationWindow
// padding to the window's safe area. The value is displayed in Settings next to
// the "Extra Top Margin" spinbox so a user can see what the platform reports.
// See docs/android-edge-to-edge-and-safe-areas.md
int get_status_bar_height() {
#ifdef Q_OS_ANDROID
    // Get the status bar height from Android system resources
    QJniEnvironment env;
    QJniObject activity = QJniObject::callStaticObjectMethod(
        "org/qtproject/qt/android/QtNative",
        "activity",
        "()Landroid/app/Activity;"
    );

    if (activity.isValid()) {
        // Get the Resources object
        QJniObject resources = activity.callObjectMethod(
            "getResources",
            "()Landroid/content/res/Resources;"
        );

        if (resources.isValid()) {
            // Get resource ID for status_bar_height
            jint resourceId = resources.callMethod<jint>(
                "getIdentifier",
                "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)I",
                QJniObject::fromString("status_bar_height").object<jstring>(),
                QJniObject::fromString("dimen").object<jstring>(),
                QJniObject::fromString("android").object<jstring>()
            );

            if (resourceId > 0) {
                // Get the actual dimension value in pixels
                jint heightInPixels = resources.callMethod<jint>(
                    "getDimensionPixelSize",
                    "(I)I",
                    resourceId
                );

                // Get display metrics to convert pixels to density-independent pixels
                QJniObject displayMetrics = resources.callObjectMethod(
                    "getDisplayMetrics",
                    "()Landroid/util/DisplayMetrics;"
                );

                if (displayMetrics.isValid()) {
                    jfloat density = displayMetrics.getField<jfloat>("density");

                    // Convert pixels to density-independent pixels (dp)
                    int heightInDp = static_cast<int>(heightInPixels / density);

                    // Clear any pending JNI exceptions
                    if (env.checkAndClearExceptions()) {
                        // Exception was cleared
                    }

                    return heightInDp;
                }
            }
        }
    }

    // Clear any pending JNI exceptions
    if (env.checkAndClearExceptions()) {
        // Exception was cleared
    }

    // Default fallback value for Android if we can't get the actual height
    return 24;
#else
    // On non-Android platforms, return 0 (no status bar offset needed)
    return 0;
#endif
}

// Helper function to create storage info JSON object
QJsonObject createStorageInfo(const QString& path, const QString& internalAppDataPath) {
    QJsonObject item;
    item["path"] = path;

    // Check if internal
    bool isInternal = (path == internalAppDataPath);
    item["is_internal"] = isInternal;

    // Get storage info for the path
    QStorageInfo storage(path);

    // Set label - use displayName if available, otherwise use friendly default.
    //
    // On Android displayName() returns the volume's MOUNT POINT rather than a
    // human name — "/data/data/io.github.simsapa.app.beta",
    // "/storage/emulated" — so it is never empty and the friendly fallbacks
    // below would never fire. A path-shaped label rendered directly above the
    // row's full path is redundant and tells the user nothing, so a label that
    // *is* a path — it starts with '/', or the candidate path starts with it —
    // is treated as no label at all.
    //
    // The prefix test alone is not enough: the internal candidate's path is
    // /data/user/0/<pkg>/files while displayName() reports
    // /data/data/<pkg>, and those are the same directory only by way of a
    // symlink, so neither string is a prefix of the other. Verified on device.
    //
    // A real volume name (a card's FAT label, say) does not start with '/' and
    // is still used. See docs/relocated-storage-recovery.md.
    QString label = storage.displayName();
    if (!label.isEmpty() && (label.startsWith('/') || path.startsWith(label))) {
        label.clear();
    }
    if (label.isEmpty()) {
        // Provide user-friendly labels when displayName is not available
        if (isInternal) {
            label = "Internal Storage";
        } else {
            // Check if it looks like an SD card path
            if (path.contains("/storage/") && !path.contains("/emulated/")) {
                label = "SD Card";
            } else {
                label = "External Storage";
            }
        }
    }
    item["label"] = label;

    // Storage sizes in megabytes
    item["megabytes_total"] = static_cast<int>(storage.bytesTotal() / (1024 * 1024));
    item["megabytes_available"] = static_cast<int>(storage.bytesAvailable() / (1024 * 1024));

    // Tier-1 usability. Defaults to usable; the caller demotes external
    // candidates from the volume's mounted state. The expensive write / SQLite
    // probe is tier 2 and deliberately NOT done here — this function is called
    // from the storage scan and from StorageDialog's Component.onCompleted,
    // i.e. from inside the QML engine load.
    // See docs/relocated-storage-recovery.md.
    item["is_usable"] = true;
    item["unusable_reason"] = "";

    return item;
}

#ifdef Q_OS_ANDROID
// The mounted state of the volume containing `path`, via
// Environment.getExternalStorageState(File) — "mounted", "mounted_ro",
// "removed", "unmounted", "bad_removal", … (API 19).
//
// External candidates only: the internal app-data directory is present and
// writable by definition, and this call is not meaningful for it.
static QString android_external_storage_state(const QString& path) {
    QJniObject file_obj(
        "java/io/File",
        "(Ljava/lang/String;)V",
        QJniObject::fromString(path).object<jstring>());

    if (!file_obj.isValid()) {
        return QString();
    }

    QJniObject state = QJniObject::callStaticObjectMethod(
        "android/os/Environment",
        "getExternalStorageState",
        "(Ljava/io/File;)Ljava/lang/String;",
        file_obj.object<jobject>());

    return state.isValid() ? state.toString() : QString();
}

// Whether the external volume containing `path` is *emulated* — i.e. backed by
// the device's own internal storage rather than by a removable card
// (Environment.isExternalStorageEmulated(File), API 21).
//
// This is the fact that identifies the duplicate: on a phone with no card,
// /storage/emulated/0/Android/data/<pkg>/files and the internal app-data
// directory are two views of the SAME physical storage, and they report
// identical total and available bytes. Listing both as separate choices offers
// the user a decision with no consequence.
static bool android_external_storage_is_emulated(const QString& path) {
    QJniObject file_obj(
        "java/io/File",
        "(Ljava/lang/String;)V",
        QJniObject::fromString(path).object<jstring>());

    if (!file_obj.isValid()) {
        return false;
    }

    return QJniObject::callStaticMethod<jboolean>(
        "android/os/Environment",
        "isExternalStorageEmulated",
        "(Ljava/io/File;)Z",
        file_obj.object<jobject>());
}

// Whether the external volume containing `path` is removable
// (Environment.isExternalStorageRemovable(File), API 21) — a real card slot or
// USB volume, as opposed to emulated internal storage.
static bool android_external_storage_is_removable(const QString& path) {
    QJniObject file_obj(
        "java/io/File",
        "(Ljava/lang/String;)V",
        QJniObject::fromString(path).object<jstring>());

    if (!file_obj.isValid()) {
        return false;
    }

    return QJniObject::callStaticMethod<jboolean>(
        "android/os/Environment",
        "isExternalStorageRemovable",
        "(Ljava/io/File;)Z",
        file_obj.object<jobject>());
}

// Append a row for every volume android.os.storage.StorageManager reports that
// matches none of the already-enumerated getExternalFilesDirs() entries.
//
// These are volumes the app can SEE but cannot write app data to — typically
// USB / SAF-only storage. Reporting them is the point: a volume the user can
// see in their phone but that silently vanishes from the app's list is exactly
// the confusion this feature exists to remove. SQLite needs a real filesystem
// path, so a SAF-only volume is genuinely unusable for the database.
//
// Matching is deliberately conservative (FR-33's safe direction): only volumes
// matching NOTHING become extra rows, so a matching failure can produce a
// missing warning but never a duplicated or wrongly-disabled usable location.
// All strategies are API 24 or earlier — StorageVolume.getDirectory() is API 30
// and must NOT be used at minSdk 27:
//
//   - getStorageVolume(File) + StorageVolume.equals() (API 24): the volume the
//     platform itself says a candidate path lives on. Exact, and independent of
//     paths, uuids and labels — this is the primary test,
//   - UUID appearing in an enumerated path (ordinary FAT/exFAT cards),
//   - isPrimary() (primary emulated storage: getUuid() returns null),
//   - getDescription(Context) matching a row label.
//
// The last three are fallbacks for the case getStorageVolume() returns null
// (a path on no reported volume). The description test in particular can no
// longer fire on its own: createStorageInfo() discards path-shaped labels and
// substitutes "Internal Storage" / "SD Card" / "External Storage", which a
// volume description will essentially never equal. That is what the exact test
// above replaces — before it, an adopted (internal-formatted) volume matched
// nothing and would have been reported as a bogus "Not usable for app data"
// row. Kept anyway: it costs nothing and can still fire for a card whose FAT
// label survives into the row label.
// The StorageVolume containing `path`, via
// android.os.storage.StorageManager.getStorageVolume(File) (API 24). Returns an
// invalid object when the path is on no reported volume.
static QJniObject android_storage_volume_for_path(const QJniObject& storage_manager,
                                                  const QString& path) {
    if (path.isEmpty()) {
        return QJniObject();
    }

    QJniObject file_obj(
        "java/io/File",
        "(Ljava/lang/String;)V",
        QJniObject::fromString(path).object<jstring>());

    if (!file_obj.isValid()) {
        return QJniObject();
    }

    return storage_manager.callObjectMethod(
        "getStorageVolume",
        "(Ljava/io/File;)Landroid/os/storage/StorageVolume;",
        file_obj.object<jobject>());
}

static void append_unmatched_storage_volumes(QJsonArray& storageArray) {
    QJniEnvironment env;

    QJniObject activity = QJniObject::callStaticObjectMethod(
        "org/qtproject/qt/android/QtNative",
        "activity",
        "()Landroid/app/Activity;");

    if (!activity.isValid()) {
        return;
    }

    QJniObject storage_manager = activity.callObjectMethod(
        "getSystemService",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        QJniObject::fromString("storage").object<jstring>());

    if (!storage_manager.isValid()) {
        log_error_c("getStorageVolumes pass: STORAGE_SERVICE unavailable");
        env.checkAndClearExceptions();
        return;
    }

    QJniObject volumes = storage_manager.callObjectMethod(
        "getStorageVolumes",
        "()Ljava/util/List;");

    if (!volumes.isValid()) {
        log_error_c("getStorageVolumes pass: getStorageVolumes() returned nothing");
        env.checkAndClearExceptions();
        return;
    }

    const jint count = volumes.callMethod<jint>("size", "()I");
    log_info_c(QString("getStorageVolumes pass: %1 volume(s) reported, %2 enumerated candidate(s)")
                   .arg(count)
                   .arg(storageArray.size())
                   .toUtf8()
                   .constData());

    // The volume each already-enumerated candidate lives on, resolved once.
    // Comparing these with StorageVolume.equals() is the exact match; the
    // uuid / isPrimary / description tests below are fallbacks for paths the
    // platform maps to no volume.
    QList<QJniObject> row_volumes;
    row_volumes.reserve(storageArray.size());
    for (int r = 0; r < storageArray.size(); ++r) {
        row_volumes.append(android_storage_volume_for_path(
            storage_manager, storageArray.at(r).toObject().value("path").toString()));
    }
    env.checkAndClearExceptions();

    for (jint i = 0; i < count; ++i) {
        QJniObject volume = volumes.callObjectMethod("get", "(I)Ljava/lang/Object;", i);
        if (!volume.isValid()) {
            continue;
        }

        const bool is_primary = volume.callMethod<jboolean>("isPrimary", "()Z");

        QJniObject uuid_obj = volume.callObjectMethod("getUuid", "()Ljava/lang/String;");
        const QString uuid = uuid_obj.isValid() ? uuid_obj.toString() : QString();

        QJniObject description_obj = volume.callObjectMethod(
            "getDescription",
            "(Landroid/content/Context;)Ljava/lang/String;",
            activity.object<jobject>());
        const QString description = description_obj.isValid() ? description_obj.toString() : QString();

        // The exact test first: is this the volume the platform itself reports
        // for one of the enumerated candidate paths?
        bool matched = false;
        QString matched_by;
        for (int r = 0; r < row_volumes.size(); ++r) {
            if (!row_volumes.at(r).isValid()) {
                continue;
            }
            if (row_volumes.at(r).callMethod<jboolean>(
                    "equals", "(Ljava/lang/Object;)Z", volume.object<jobject>())) {
                matched = true;
                matched_by = QStringLiteral("volume");
                break;
            }
        }

        // Primary emulated storage: getUuid() returns null, so it can only be
        // matched by this flag.
        if (!matched && is_primary) {
            matched = true;
            matched_by = QStringLiteral("primary");
        }

        for (int r = 0; !matched && r < storageArray.size(); ++r) {
            const QJsonObject row = storageArray.at(r).toObject();
            const QString row_path = row.value("path").toString();
            const QString row_label = row.value("label").toString();

            if (!uuid.isEmpty() && row_path.contains(uuid)) {
                matched = true;
                matched_by = QStringLiteral("uuid-in-path");
                break;
            }
            if (!description.isEmpty() && row_label == description) {
                matched = true;
                matched_by = QStringLiteral("description");
                break;
            }
        }

        // Logged for every volume, matched or not. A JNI signature error here
        // fails silently — the exception is cleared and an invalid object comes
        // back — which is indistinguishable from "matched everything", and on a
        // phone with no removable storage the correct answer is also "no extra
        // rows". Without this line a passing test proves nothing.
        //
        // matched_by names which test fired, so a device run shows whether the
        // exact getStorageVolume() match is working ("volume") or whether the
        // older heuristics are carrying it.
        log_info_c(QString("StorageVolume: uuid=%1 description=%2 primary=%3 matched=%4 by=%5")
                       .arg(uuid.isEmpty() ? QStringLiteral("(none)") : uuid)
                       .arg(description.isEmpty() ? QStringLiteral("(none)") : description)
                       .arg(is_primary ? "true" : "false")
                       .arg(matched ? "true" : "false")
                       .arg(matched_by.isEmpty() ? QStringLiteral("(none)") : matched_by)
                       .toUtf8()
                       .constData());

        if (matched) {
            continue;
        }

        QJsonObject item;
        // No usable path: this volume has no app-writable directory, which is
        // the whole reason it is being reported.
        item["path"] = "";
        item["label"] = description.isEmpty() ? QString("External Storage") : description;
        item["is_internal"] = false;
        item["megabytes_total"] = 0;
        item["megabytes_available"] = 0;
        item["is_usable"] = false;
        item["unusable_reason"] =
            "Not usable for app data (this device may only allow file transfers here)";
        storageArray.append(item);
    }

    env.checkAndClearExceptions();
}
#endif

QJsonArray get_app_data_storage_paths() {
    QJsonArray storageArray;

    // Get internal app data path (common for all platforms)
    QString internalAppDataPath = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation);

    // Add internal storage path
    if (!internalAppDataPath.isEmpty()) {
        storageArray.append(createStorageInfo(internalAppDataPath, internalAppDataPath));
    }

#ifdef Q_OS_ANDROID
    // On Android, get external storage paths
    QJniEnvironment env;
    QJniObject activity = QJniObject::callStaticObjectMethod(
        "org/qtproject/qt/android/QtNative",
        "activity",
        "()Landroid/app/Activity;"
    );

    if (activity.isValid()) {
        // Call getExternalFilesDirs(null) to get all external storage paths
        QJniObject externalDirs = activity.callObjectMethod(
            "getExternalFilesDirs",
            "(Ljava/lang/String;)[Ljava/io/File;",
            nullptr
        );

        if (externalDirs.isValid()) {
            // Get the array length
            jsize length = env->GetArrayLength(externalDirs.object<jobjectArray>());

            for (int i = 0; i < length; ++i) {
                QJniObject fileObject = env->GetObjectArrayElement(
                    externalDirs.object<jobjectArray>(),
                    i
                );

                if (fileObject.isValid()) {
                    // Get the absolute path of the File object
                    QJniObject pathObject = fileObject.callObjectMethod(
                        "getAbsolutePath",
                        "()Ljava/lang/String;"
                    );

                    if (pathObject.isValid()) {
                        QString externalPath = pathObject.toString();

                        // Only add if it's different from internal path and not empty
                        if (!externalPath.isEmpty() && externalPath != internalAppDataPath) {
                            QJsonObject item = createStorageInfo(externalPath, internalAppDataPath);

                            // Is this its own physical storage, or another view
                            // of the internal one? Reported as facts; the
                            // de-duplication policy lives in the Rust scan.
                            const bool is_emulated = android_external_storage_is_emulated(externalPath);
                            const bool is_removable = android_external_storage_is_removable(externalPath);
                            item["is_emulated"] = is_emulated;
                            item["is_removable"] = is_removable;

                            log_info_c(QString("External candidate: %1 emulated=%2 removable=%3")
                                           .arg(externalPath)
                                           .arg(is_emulated ? "true" : "false")
                                           .arg(is_removable ? "true" : "false")
                                           .toUtf8()
                                           .constData());

                            // Tier-1 classification, external candidates only.
                            const QString state = android_external_storage_state(externalPath);
                            if (state == QLatin1String("mounted_ro")) {
                                item["is_usable"] = false;
                                item["unusable_reason"] = "Read-only — the app cannot write here";
                            } else if (!state.isEmpty() && state != QLatin1String("mounted")) {
                                item["is_usable"] = false;
                                item["unusable_reason"] = "Not available";
                            }

                            storageArray.append(item);
                        }
                    }
                }
            }
        }
    }

    // Volumes the app can see but has no app-writable directory on.
    append_unmatched_storage_volumes(storageArray);

    // Clear any pending JNI exceptions
    env.checkAndClearExceptions();
#endif

    return storageArray;
}

QString get_app_data_storage_paths_json() {
    QJsonArray storageArray = get_app_data_storage_paths();
    QJsonDocument doc(storageArray);
    return doc.toJson(QJsonDocument::Compact);
}

QString copy_file(QString source_file, QString destination_file) {
    QFileInfo fileInfo(source_file);
    if (fileInfo.isDir()) {
        return QString("Error: Is a directory: " + source_file);
    }

    QDir dest_dir = QFileInfo(destination_file).dir();
    if (!dest_dir.exists()) {
        if (!dest_dir.mkpath(".")) {
            QString ret_msg = QString("Failed to create directory for: " + destination_file);
            log_error_c(ret_msg.toUtf8().constData());
            return ret_msg;
        }
    }

    QFile source(source_file);
    if (!source.copy(destination_file)) {
        QString ret_msg("Failed to copy file: " + source_file + ", error: " + source.errorString());
        log_error_c(ret_msg.toUtf8().constData());
        return ret_msg;
    }

    QFile::setPermissions(destination_file,
        QFileDevice::ReadUser |
        QFileDevice::WriteUser |
        QFileDevice::ReadOwner |
        QFileDevice::WriteOwner);

    return QString("");
}

// The folder where picked files are staged before an import. This is the single
// source of truth for the C++ side; the Rust cleanup uses std::env::temp_dir(),
// which is not guaranteed to be the same directory on Android. See
// docs/file-selection-test.md.
QString get_import_staging_root() {
    return QStandardPaths::writableLocation(QStandardPaths::TempLocation) + "/simsapa-imports";
}

QString copy_content_uri_to_temp_file(const QString& content_uri) {
#ifdef Q_OS_ANDROID
    // Only handle content:// URIs
    if (!content_uri.startsWith("content://")) {
        return QString("");
    }

    // Try to resolve the user-visible filename via ContentResolver
    // (OpenableColumns.DISPLAY_NAME). The URI's last path segment is often an
    // opaque id like "msf%3A1003", so falling back to it loses the original
    // name (e.g. "mw-gd.zip" becomes "msf_3A1003").
    QString filename;
    {
        QJniObject activity = QJniObject::callStaticObjectMethod(
            "org/qtproject/qt/android/QtNative",
            "activity",
            "()Landroid/app/Activity;");
        QJniObject resolver = activity.isValid()
            ? activity.callObjectMethod("getContentResolver", "()Landroid/content/ContentResolver;")
            : QJniObject();
        QJniObject uri_obj = QJniObject::callStaticObjectMethod(
            "android/net/Uri",
            "parse",
            "(Ljava/lang/String;)Landroid/net/Uri;",
            QJniObject::fromString(content_uri).object<jstring>());

        if (resolver.isValid() && uri_obj.isValid()) {
            QJniEnvironment env;
            jobjectArray projection = env->NewObjectArray(
                1, env->FindClass("java/lang/String"),
                QJniObject::fromString("_display_name").object<jstring>());

            QJniObject cursor = resolver.callObjectMethod(
                "query",
                "(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;",
                uri_obj.object(), projection, nullptr, nullptr, nullptr);
            env->DeleteLocalRef(projection);

            if (cursor.isValid() && cursor.callMethod<jboolean>("moveToFirst")) {
                QJniObject name_obj = cursor.callObjectMethod("getString", "(I)Ljava/lang/String;", 0);
                if (name_obj.isValid()) {
                    filename = name_obj.toString();
                }
            }
            if (cursor.isValid()) {
                cursor.callMethod<void>("close");
            }
        }
    }

    if (filename.isEmpty()) {
        // Fallback: last URI segment, or a generic name.
        filename = content_uri.section('/', -1);
        if (filename.isEmpty()) {
            filename = "imported_file";
        }
    }

    // Create temp directory
    QString temp_dir = get_import_staging_root();
    QDir dir;
    if (!dir.mkpath(temp_dir)) {
        log_error_c(QString("Failed to create temp directory: %1").arg(temp_dir).toUtf8().constData());
        return QString("");
    }

    QString temp_path = temp_dir + "/" + filename;

    // Open the content URI for reading
    QFile source(content_uri);
    if (!source.open(QIODevice::ReadOnly)) {
        log_error_c(QString("Failed to open content URI: %1 Error: %2")
                    .arg(content_uri, source.errorString()).toUtf8().constData());
        return QString("");
    }

    // Open destination file for writing
    QFile dest(temp_path);
    if (!dest.open(QIODevice::WriteOnly)) {
        log_error_c(QString("Failed to create temp file: %1 Error: %2")
                    .arg(temp_path, dest.errorString()).toUtf8().constData());
        source.close();
        return QString("");
    }

    // Copy data
    QByteArray data = source.readAll();
    if (data.isEmpty() && source.error() != QFile::NoError) {
        log_error_c(QString("Failed to read from content URI: %1")
                    .arg(source.errorString()).toUtf8().constData());
        source.close();
        dest.close();
        return QString("");
    }

    qint64 written = dest.write(data);
    source.close();
    dest.close();

    if (written != data.size()) {
        log_error_c(QString("Failed to write all data to temp file: %1 (wrote %2 of %3 bytes)")
                    .arg(temp_path).arg(written).arg(data.size()).toUtf8().constData());
        QFile::remove(temp_path);
        return QString("");
    }

    log_info_c(QString("Copied content URI to temp file: %1").arg(temp_path).toUtf8().constData());
    return temp_path;
#else
    // On non-Android platforms, content:// URIs shouldn't occur
    Q_UNUSED(content_uri);
    return QString("");
#endif
}

// The package name this app is installed under, e.g. "io.github.simsapa.app"
// or "io.github.simsapa.app.beta" for a beta build. Empty off Android.
QString get_android_package_name() {
#ifdef Q_OS_ANDROID
    QJniEnvironment env;
    QJniObject activity = QJniObject::callStaticObjectMethod(
        "org/qtproject/qt/android/QtNative",
        "activity",
        "()Landroid/app/Activity;");

    if (activity.isValid()) {
        QJniObject name = activity.callObjectMethod("getPackageName", "()Ljava/lang/String;");
        if (name.isValid()) {
            env.checkAndClearExceptions();
            return name.toString();
        }
    }

    env.checkAndClearExceptions();
    return QString();
#else
    return QString();
#endif
}

// Which store or app installed this copy: "com.android.vending" for Google
// Play, something else for another store, and empty for a sideloaded APK (or
// off Android entirely).
//
// This is what decides whether the in-app update notice may offer a download
// link. An app distributed THROUGH Play must not update itself from anywhere
// else — Play's Device and Network Abuse policy — so a Play-installed copy is
// sent to its Play listing instead. A sideloaded copy (a GitHub Releases beta,
// say) is not covered by that policy and keeps the direct link.
//
// Deliberately a property of the INSTALL, not of the build: a release APK
// downloaded from GitHub and sideloaded is the same artifact that Play serves,
// and it should get the link. Only the copy that actually came from Play is
// restricted.
//
// getInstallSourceInfo() replaced getInstallerPackageName() in API 30. The
// latter is deprecated but still functional and works on every level the app
// supports (minSdk 27), so it is used directly rather than branched on.
QString get_installer_package_name() {
#ifdef Q_OS_ANDROID
    QJniEnvironment env;
    QJniObject activity = QJniObject::callStaticObjectMethod(
        "org/qtproject/qt/android/QtNative",
        "activity",
        "()Landroid/app/Activity;");

    if (activity.isValid()) {
        QJniObject package_manager = activity.callObjectMethod(
            "getPackageManager",
            "()Landroid/content/pm/PackageManager;");
        QJniObject package_name = activity.callObjectMethod(
            "getPackageName",
            "()Ljava/lang/String;");

        if (package_manager.isValid() && package_name.isValid()) {
            QJniObject installer = package_manager.callObjectMethod(
                "getInstallerPackageName",
                "(Ljava/lang/String;)Ljava/lang/String;",
                package_name.object<jstring>());

            // Returns null for a sideloaded package; QJniObject wraps that as
            // an invalid object rather than an empty string.
            if (installer.isValid()) {
                env.checkAndClearExceptions();
                return installer.toString();
            }
        }
    }

    env.checkAndClearExceptions();
    return QString();
#else
    return QString();
#endif
}

QString get_qt_platform_name() {
    if (QGuiApplication::instance()) {
        return QGuiApplication::platformName();
    }
    return QString();
}

QString get_qt_version() {
    // Runtime Qt version string, e.g. "6.9.3".
    return QString::fromLatin1(qVersion());
}

QString copy_apk_assets_to_internal_storage(QString apk_asset_path /* = QString("") */) {
    QString assets_storage = get_app_assets_path();
    QString ret_msg = QString("");

    QDir assets_storage_dir(assets_storage);
    if (!assets_storage_dir.exists()) {
        if (!assets_storage_dir.mkpath(".")) {
            ret_msg = QString("Failed to create directory: " + assets_storage);
            qWarning() << ret_msg;
            return ret_msg;
        }
    }

    // If no specific path provided, copy all assets
    if (apk_asset_path.isEmpty()) {
        apk_asset_path = "/";
    }

    // Create destination directory for the asset path
    QString dest_dir_path = assets_storage + apk_asset_path;
    QDir dest_dir(dest_dir_path);
    if (!dest_dir.exists()) {
        if (!dest_dir.mkpath(".")) {
            ret_msg = QString("Failed to create directory: " + dest_dir_path);
            qWarning() << ret_msg;
            return ret_msg;
        }
    }

    QDir apk_assets_dir("assets:" + apk_asset_path);
    if (apk_assets_dir.exists()) {
        // Copy directory contents recursively
        QStringList entries = apk_assets_dir.entryList(QDir::AllEntries | QDir::Hidden | QDir::System);

        foreach (const QString& entry, entries) {
            if (entry == "." || entry == "..") {
                continue;
            }

            QString source_path = "assets:" + apk_asset_path + "/" + entry;
            QString destination_file = dest_dir_path + "/" + entry;

            QFileInfo fileInfo(source_path);
            if (fileInfo.isDir()) {
                // Recursive directory copy
                QString r = copy_apk_assets_to_internal_storage(apk_asset_path + "/" + entry);
                if (!r.isEmpty()) {
                    qWarning() << r;
                    return r;
                }
            } else {
                copy_file(source_path, destination_file);
            }
        }

    } else {
        // Handle single file copy
        QString source_path = "assets:" + apk_asset_path;
        QString destination_file = dest_dir_path;

        copy_file(source_path, destination_file);
    }

    return ret_msg;
}
