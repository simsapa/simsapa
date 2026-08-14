#include "screen.h"

// Q_OS_ANDROID comes from Qt's qsystemdetection.h, not from the build system.
// Without this include the #ifdef below is silently false on Android and
// keep_screen_on() compiles down to the desktop no-op -- the device would then
// suspend part-way through a download or an index rebuild, with the log line
// still claiming the flag had been set.
#include <QtGlobal>

#include <QSet>
#include <QStringList>

#ifdef Q_OS_ANDROID
#include <QCoreApplication>
#include <QJniObject>
#include <QVariant>

// android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON
static constexpr jint FLAG_KEEP_SCREEN_ON = 0x00000080;
#endif

extern "C" void log_info_c(const char* msg);
extern "C" void log_error_c(const char* msg);

/* Why this is a set of named holders and not a plain bool.
 *
 * FLAG_KEEP_SCREEN_ON is ONE boolean on the single Android Activity window.
 * Every caller in the app shares it, and there are two kinds of caller:
 *
 *   - window-scoped: SuttaLanguagesWindow, DownloadAppdataWindow and
 *     ChantingPracticeReviewWindow hold it for as long as the window is open;
 *   - operation-scoped: the search-index rebuild (from two different screens),
 *     the language download/import, storage diagnostics, the topic-index
 *     update and the file-selection test hold it for as long as the job runs.
 *
 * These overlap by construction -- a download runs *inside* the languages
 * window, storage diagnostics can be started while it is open -- so with a bare
 * on/off flag the first release cancels everyone else's request. (That hazard
 * is structural rather than a specific measured incident: the release seen on
 * device right after a language import is equally explained by the window
 * being destroyed on completion, which legitimately releases its own hold.)
 *
 * What *was* measured is the other failure mode this prevents: an unreleased
 * hold. DatabaseValidationDialog embeds a permanently hidden
 * DownloadAppdataWindow, whose lifetime hold was never released, so the screen
 * could not sleep on a healthy install. Naming the holders is what identified
 * it from a log, with no reproduction.
 *
 * A plain reference count would fix the overlap but not the asymmetry: a
 * missing release leaks a count that nothing can ever recover, and a double
 * release silently steals someone else's hold. Naming each holder makes both
 * harmless -- acquiring twice is idempotent, releasing something you do not
 * hold is a logged no-op -- and makes the log say *who* is keeping the screen
 * awake, which a counter cannot.
 *
 * The flag is only added when the set becomes non-empty, and only cleared when
 * it becomes empty again.
 *
 * Callers are on the Qt GUI thread (QML). The JNI call is dispatched to the
 * Android UI thread, but the set is only touched here.
 */
static QSet<QString> g_screen_lock_holders;

#ifdef Q_OS_ANDROID
static void apply_screen_flag(bool on) {
    // Window-flag changes must happen on the Android UI (main) thread.
    // Use the public QNativeInterface API (no private QtCore headers needed).
    QNativeInterface::QAndroidApplication::runOnAndroidMainThread([on]() -> QVariant {
        QJniObject activity = QNativeInterface::QAndroidApplication::context();
        if (!activity.isValid()) {
            log_error_c("keep_screen_on: failed to get Android activity");
            return QVariant();
        }

        QJniObject window = activity.callObjectMethod(
            "getWindow", "()Landroid/view/Window;");
        if (!window.isValid()) {
            log_error_c("keep_screen_on: failed to get activity window");
            return QVariant();
        }

        if (on) {
            window.callMethod<void>("addFlags", "(I)V", FLAG_KEEP_SCREEN_ON);
            log_info_c("keep_screen_on: FLAG_KEEP_SCREEN_ON added");
        } else {
            window.callMethod<void>("clearFlags", "(I)V", FLAG_KEEP_SCREEN_ON);
            log_info_c("keep_screen_on: FLAG_KEEP_SCREEN_ON cleared");
        }
        return QVariant();
    });
}
#endif

// Every line names the holder and lists who still holds it, so an unreleased
// hold can be identified from a user's log.txt without a reproduction.
static void log_holders(const QString& prefix) {
    QStringList names(g_screen_lock_holders.constBegin(), g_screen_lock_holders.constEnd());
    names.sort();
    const QString msg = names.isEmpty()
        ? prefix + QStringLiteral(" - no holders remain")
        : prefix + QStringLiteral(" - holders: ") + names.join(QStringLiteral(", "));
    log_info_c(msg.toUtf8().constData());
}

void keep_screen_on(const QString& holder, bool on) {
    const QString name = holder.trimmed().isEmpty() ? QStringLiteral("unnamed") : holder.trimmed();

    if (on) {
        if (g_screen_lock_holders.contains(name)) {
            // Idempotent: a window that re-acquires on every show, for
            // instance, must not need a matching extra release.
            log_holders(QStringLiteral("keep_screen_on(true) by '%1' - already held").arg(name));
            return;
        }

        const bool was_empty = g_screen_lock_holders.isEmpty();
        g_screen_lock_holders.insert(name);
        log_holders(QStringLiteral("keep_screen_on(true) by '%1'").arg(name));

        if (!was_empty) {
            // Someone else already has the flag set; nothing to do at the OS
            // level, and re-adding it would be a no-op anyway.
            return;
        }
    } else {
        if (!g_screen_lock_holders.remove(name)) {
            // Not an error the user can act on, but it is always a bug in the
            // calling code: a release with no matching acquire, or a second
            // release. Logged rather than silently ignored.
            log_error_c(QStringLiteral("keep_screen_on(false) by '%1' - was not holding it, ignoring")
                            .arg(name).toUtf8().constData());
            return;
        }

        log_holders(QStringLiteral("keep_screen_on(false) by '%1'").arg(name));

        if (!g_screen_lock_holders.isEmpty()) {
            // Someone else still wants the screen awake. This is the case the
            // shared boolean got wrong.
            return;
        }
    }

#ifdef Q_OS_ANDROID
    apply_screen_flag(on);
#else
    log_info_c("keep_screen_on() - not on Android platform, no-op");
#endif
}
