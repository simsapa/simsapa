// Capture the file picker's URI as the raw Java string, before Qt converts it.
//
// WHY THIS EXISTS
//
// A Chromebook user cannot import a StarDict .zip: the log records
// `Path not found: ` with nothing after the colon, so QML was handed an empty
// URL. Qt's own Android file-dialog helper explains how that happens
// (qandroidplatformfiledialoghelper.cpp, handleActivityResult):
//
//     m_selectedFile.append(QUrl(uri.toString()));
//     Q_EMIT accept();
//
// The Java Uri's string is handed to the QUrl(QString) constructor, and accept()
// is emitted whatever that produced. If the ARC picker's URI does not parse as a
// QUrl, QML's onAccepted fires with an empty selectedFile — and the whole file
// emits no warning of any kind. currentFile, currentFiles and selectedFiles are
// all fed from that same list, so on this path they are empty together.
//
// The raw string is therefore destroyed inside Qt before any app code runs. A
// diagnostic built on Qt's FileDialog can confirm "empty" — which the user's log
// already told us — and learn nothing more. Running our own ACTION_OPEN_DOCUMENT
// is the only way to see what the picker actually returned.
//
// WHY PRIVATE QT API IS ACCEPTABLE HERE
//
// This uses QtCore/private/qandroidextras_p.h, which is what Qt's own file
// dialog helper uses. The decision, and the evidence for it, is recorded in the
// PRD (tasks/2026-07-31-180502-prd---picker-url-handling-and-chromebook-import-failure.md
// §11 Q0a). In short, checked against qtbase's 6.11 branch — the release this
// project is upgrading to:
//
//   - the API is unchanged there: QAndroidActivityResultReceiver and all three
//     QtAndroidPrivate::startActivity overloads carry signatures identical to
//     the 6.9.3 kit, and the header's most recent commit is cosmetic;
//   - Qt6::CorePrivate is an *interface* target — include paths only. No new .so
//     in the package, no ABI-slice growth, no manifest change;
//   - a future break would be a compile error, not silent misbehaviour.
//
// THIS IS NO LONGER DIAGNOSTIC-ONLY
//
// The two rules that used to stand here — "delete it once the report comes
// back" and "do not let the import path depend on it" — were written when this
// file served one test button. The report came back and settled the question the
// other way: on the reporting Chromebook every pick through this intent worked
// perfectly, while Qt's FileDialog returned nothing. So the dictionary import
// now falls back to this picker when Qt's chooser hands it an empty URL, and the
// alternative to that dependency is a user who cannot import a dictionary at
// all. The decision, and its terms, are recorded in the PRD §11 Q0a.
//
// The terms the reversal keeps:
//
//   1. **The private include stays in this file alone.** The blast radius is
//      unchanged: one translation unit, ~80 lines of #ifdef-gated code, and a
//      failure at the next Qt upgrade is a compile error, not silent
//      misbehaviour.
//   2. **One report shape.** Both callers feed the same PickerUrlFacts pipeline
//      in backend/src/picker_url.rs; the import's block differs only by its
//      prefix (DICTIONARY-IMPORT-PICK:) and by not reading the document.
//   3. **One global result slot, with a discriminator.** The consumer is
//      recorded in RAW_PICK_TARGET (bridges/src/sutta_bridge.rs) when a pick
//      starts, so a diagnostic pick and an import pick cannot be delivered to
//      the wrong listener. Do not add a second parallel mechanism.

#include "android_raw_pick.h"

#include <QString>

#ifdef Q_OS_ANDROID
#include <QJniObject>
#include <QJniEnvironment>
#include <QtCore/private/qandroidextras_p.h>
#endif

// The app's own logger. On Android, Qt's qInfo()/qWarning() are tagged with the
// application name rather than "Qt", so they fall outside the documented
// `adb logcat -s simsapa Qt QtCore QtQml` tag set and never reach log.txt --
// which is the file the reporting user is asked to send.
extern "C" void log_info_c(const char* msg);
extern "C" void log_error_c(const char* msg);

// Implemented in Rust (backend/src/picker_url.rs), the same C-ABI shape as
// log_info_c(). `source` names which branch of the result produced the string,
// so one report can say how the block was obtained.
extern "C" void raw_document_pick_result_c(const char* raw_uri, const char* source);

#ifdef Q_OS_ANDROID
namespace {

// Qt's own file dialog uses 1305 (qandroidplatformfiledialoghelper.cpp). Ours
// must not collide with it: the result is dispatched by request code, and a
// clash would cross the two dialogs' results over.
constexpr int RAW_PICK_REQUEST_CODE = 51305;

// android.app.Activity.RESULT_OK
constexpr int RESULT_OK = -1;

void deliver(const QString& raw_uri, const char* source) {
    log_info_c(QString("start_raw_document_pick(): result via %1, %2 chars")
               .arg(QString::fromUtf8(source))
               .arg(raw_uri.length())
               .toUtf8().constData());
    raw_document_pick_result_c(raw_uri.toUtf8().constData(), source);
}

// Read the URI the picker returned, without letting it near a QUrl.
//
// Qt's helper handles getData() and getClipData() and silently does nothing when
// there is neither. The "neither" case is reported here rather than dropped: a
// pick that returns no URI at all is a finding, and a diagnostic that produced
// no block would be indistinguishable from a button that did not work.
void handle_result(int result_code, const QJniObject& data) {
    QJniEnvironment env;

    if (result_code != RESULT_OK) {
        deliver(QString(), "cancelled");
        return;
    }

    if (!data.isValid()) {
        deliver(QString(), "no-intent");
        return;
    }

    QJniObject uri = data.callObjectMethod("getData", "()Landroid/net/Uri;");
    if (uri.isValid()) {
        // toString() on the *Java* Uri, kept as a Java string. This is the value
        // Qt would have handed to QUrl(QString), and the single most valuable
        // line in the report.
        //
        // Any pending JNI exception is cleared *before* delivering: the delivery
        // crosses into Rust, and leaving an exception pending across that
        // boundary makes the next JNI call in any thread misbehave.
        const QString raw_uri = uri.callObjectMethod<jstring>("toString").toString();
        env.checkAndClearExceptions();
        deliver(raw_uri, "intent-getData");
        return;
    }

    QJniObject clip = data.callObjectMethod("getClipData", "()Landroid/content/ClipData;");
    if (clip.isValid() && clip.callMethod<jint>("getItemCount") > 0) {
        QJniObject item = clip.callObjectMethod("getItemAt",
                                                "(I)Landroid/content/ClipData$Item;",
                                                0);
        QJniObject clip_uri = item.isValid()
            ? item.callObjectMethod("getUri", "()Landroid/net/Uri;")
            : QJniObject();
        if (clip_uri.isValid()) {
            const QString raw_uri = clip_uri.callObjectMethod<jstring>("toString").toString();
            env.checkAndClearExceptions();
            deliver(raw_uri, "intent-getClipData");
            return;
        }
    }

    // Reached only when the picker reported success with no URI anywhere -- the
    // case Qt leaves emitting nothing at all.
    deliver(QString(), "no-uri");
    env.checkAndClearExceptions();
}

} // namespace
#endif

bool start_raw_document_pick() {
#ifdef Q_OS_ANDROID
    QJniEnvironment env;

    QJniObject action = QJniObject::fromString("android.intent.action.OPEN_DOCUMENT");
    QJniObject intent("android/content/Intent", "(Ljava/lang/String;)V", action.object<jstring>());
    if (!intent.isValid()) {
        log_error_c("start_raw_document_pick(): failed to create ACTION_OPEN_DOCUMENT intent");
        // Deliver an outcome anyway: the caller has already armed the listener
        // and disabled its button, and only a delivered result completes the run.
        raw_document_pick_result_c("", "no-intent");
        return false;
    }

    QJniObject category = QJniObject::fromString("android.intent.category.OPENABLE");
    intent.callObjectMethod("addCategory",
                            "(Ljava/lang/String;)Landroid/content/Intent;",
                            category.object<jstring>());

    // No MIME filter, for the same reason the diagnostic's own FileDialog has no
    // nameFilters: the .zip filter on the real import dialog is one of the
    // suspects, and a test that inherits the configuration it is testing cannot
    // discriminate it.
    QJniObject mime = QJniObject::fromString("*/*");
    intent.callObjectMethod("setType",
                            "(Ljava/lang/String;)Landroid/content/Intent;",
                            mime.object<jstring>());

    if (env.checkAndClearExceptions()) {
        log_error_c("start_raw_document_pick(): JNI exception while building the intent");
        raw_document_pick_result_c("", "no-intent");
        return false;
    }

    log_info_c("start_raw_document_pick(): launching ACTION_OPEN_DOCUMENT");

    // The std::function overload, so there is no receiver object whose lifetime
    // has to outlive the picker.
    QtAndroidPrivate::startActivity(
        intent,
        RAW_PICK_REQUEST_CODE,
        [](int request_code, int result_code, const QJniObject& data) {
            if (request_code != RAW_PICK_REQUEST_CODE) {
                return;
            }
            handle_result(result_code, data);
        });

    return true;
#else
    // Desktop builds neither link nor reference the private header. The report
    // says so rather than silently producing nothing.
    log_info_c("start_raw_document_pick(): not on Android, no raw pick available");
    raw_document_pick_result_c("", "unsupported-platform");
    return false;
#endif
}
