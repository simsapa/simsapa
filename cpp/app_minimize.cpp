#include "app_minimize.h"

// Q_OS_ANDROID comes from Qt's qsystemdetection.h, not from the build system.
// Without this include the #ifdef below is silently false on Android and the
// whole function compiles down to the desktop no-op.
#include <QtGlobal>

#ifdef Q_OS_ANDROID
#include <QCoreApplication>
#include <QJniObject>
#include <QVariant>
#endif

extern "C" void log_info_c(const char* msg);
extern "C" void log_error_c(const char* msg);

// Send the app to the background, leaving it in the Android overview screen
// with its windows exactly as the user left them.
//
// This is what "Close Window" does on the *last* remaining window on mobile.
// The window itself is never hidden: the session save filters on `visible`, so
// a hidden last window would write an empty session and discard the user's
// tabs. See docs/window-lifecycle-and-reuse.md.
//
// The session is not saved here. Backgrounding raises
// QApplication::applicationStateChanged, and the mobile handler for it in
// gui.cpp calls WindowManager::save_session_now() -- the same hook that covers
// a task swiped away from the overview screen, verified on device. Saving here
// as well would write the whole session twice on every minimise.
//
// iOS never reaches this function: it has no public way to background an app
// (exit(0) and the private UIApplication.suspend selector are App Store
// rejection grounds), so QML quits there instead. The branch is on the
// platform, in SuttaSearchWindow.qml's minimize_or_quit_app().
void minimize_app() {
    log_info_c("minimize_app()");
#ifdef Q_OS_ANDROID
    // moveTaskToBack() is an Activity method and must run on the Android UI
    // (main) thread. Same public QNativeInterface path as cpp/screen.cpp.
    QNativeInterface::QAndroidApplication::runOnAndroidMainThread([]() -> QVariant {
        QJniObject activity = QNativeInterface::QAndroidApplication::context();
        if (!activity.isValid()) {
            log_error_c("minimize_app: failed to get Android activity");
            return QVariant();
        }

        // moveTaskToBack(true): `true` means move the whole task back even if
        // this is not the root activity of it.
        jboolean moved = activity.callMethod<jboolean>("moveTaskToBack", "(Z)Z", JNI_TRUE);
        if (moved) {
            log_info_c("minimize_app: task moved to back");
        } else {
            log_error_c("minimize_app: moveTaskToBack() returned false, the task was not backgrounded");
        }
        return QVariant();
    });
#else
    log_info_c("minimize_app() - not on Android platform, no-op");
#endif
}
