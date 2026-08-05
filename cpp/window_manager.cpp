#include "window_manager.h"
#include "sutta_search_window.h"
#include "download_appdata_window.h"
#include "storage_recovery_window.h"
#include "sutta_languages_window.h"
#include "dictionaries_window.h"
#include "library_window.h"
#include "reference_search_window.h"
#include "topic_index_window.h"
#include "chanting_practice_window.h"
#include "chanting_review_window.h"
#include <QVariant>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QGuiApplication>
#include <QWindow>

#ifdef Q_OS_WIN
#include <windows.h>
#endif

#ifdef WITH_X11
#include "global_hotkey_manager.h"
extern "C" void x11_set_user_time(unsigned long winid, unsigned int time);
extern "C" void x11_activate_window(unsigned long winid, unsigned int time);
#endif

extern "C" void log_info_c(const char* msg);
extern "C" void log_error_c(const char* msg);

#ifdef Q_OS_MACOS
extern "C" void mac_activate_app_and_window(unsigned long long winid);
#endif

// Show + raise + activate a QML window root. Each platform requires its own
// dance on top of the portable Qt calls:
//   * Windows  -- queued global-hotkey callbacks lose the WM_HOTKEY
//                 foreground-stealing privilege, so SetForegroundWindow gets
//                 demoted to a taskbar flash. The AttachThreadInput trick
//                 temporarily shares input state with the foreground thread,
//                 restoring the privilege.
//   * macOS    -- requestActivate() only orders windows within the app; an
//                 app in the background needs an NSApplication-level activate.
//   * X11      -- _NET_ACTIVE_WINDOW is gated by EWMH focus-stealing
//                 prevention which compares the request timestamp against the
//                 user's last input time. Hotkeys delivered via XRecord have
//                 no Qt event, so without an explicit timestamp the manager
//                 sees `0` and downgrades activation to "demands attention".
//                 We feed Qt the most recent X server time captured by the
//                 hotkey worker before calling requestActivate().
static void show_and_activate_window(QObject* root) {
    if (!root) return;

    // Clear the minimized flag at the QWindow level so Qt's bookkeeping stays
    // in sync with the deminiaturize that the native activation below will
    // trigger. Without this, Qt still believes the window is minimized until
    // the WM_STATE / NSWindow notification arrives, which can race subsequent
    // show()/raise() calls. Preserves Maximized/FullScreen bits.
    //
    // On X11 specifically, this is the only Qt-side step needed: the
    // _NET_ACTIVE_WINDOW message we send afterwards with source = 2 instructs
    // the WM to deminiaturize *and* switch workspaces if the window is on
    // another desktop -- EWMH semantics every major WM (Mutter, KWin, Xfwm,
    // i3, qtile, Openbox, Fluxbox) implements as part of activation.
    if (auto* qw = qobject_cast<QWindow*>(root)) {
        Qt::WindowStates st = qw->windowStates();
        if (st & Qt::WindowMinimized) {
            qw->setWindowStates(st & ~Qt::WindowMinimized);
        }
    }

    QMetaObject::invokeMethod(root, "show");
    QMetaObject::invokeMethod(root, "raise");

#ifdef WITH_X11
    // On X11 we send _NET_ACTIVE_WINDOW ourselves with the real hotkey
    // timestamp -- Qt's requestActivate() uses its own internal time tracker
    // which is never updated for events that arrive via XRecord (no Qt event
    // is dispatched), so WMs that enforce focus-stealing prevention (qtile,
    // KWin, Mutter, ...) silently demote Qt's request to "demands attention".
    // _NET_WM_USER_TIME is also stamped for the first-map case.
    bool activated_via_x11 = false;
    if (QGuiApplication::platformName() == QLatin1String("xcb")) {
        if (auto* qw = qobject_cast<QWindow*>(root)) {
            const quint32 t = GlobalHotkeyManager::lastX11EventTime();
            const auto wid = static_cast<unsigned long>(qw->winId());
            if (wid != 0 && t != 0) {
                x11_set_user_time(wid, t);
                x11_activate_window(wid, t);
                activated_via_x11 = true;
            }
        }
    }
    if (!activated_via_x11) {
        QMetaObject::invokeMethod(root, "requestActivate");
    }
#else
    QMetaObject::invokeMethod(root, "requestActivate");
#endif

#ifdef Q_OS_MACOS
    if (auto* qw = qobject_cast<QWindow*>(root)) {
        mac_activate_app_and_window(static_cast<unsigned long long>(qw->winId()));
    } else {
        mac_activate_app_and_window(0);
    }
#endif

#ifdef Q_OS_WIN
    QWindow* qw = qobject_cast<QWindow*>(root);
    if (!qw) return;
    HWND hwnd = reinterpret_cast<HWND>(qw->winId());
    if (!hwnd) return;

    // Un-minimize first -- SetForegroundWindow doesn't restore iconic windows,
    // it just raises them in the Z-order while leaving them in the taskbar.
    if (IsIconic(hwnd)) {
        ShowWindow(hwnd, SW_RESTORE);
    }

    // Try the AttachThreadInput trick: share input state with the foreground
    // thread so SetForegroundWindow isn't demoted to a taskbar flash when the
    // WM_HOTKEY foreground-stealing privilege has expired (queued connection
    // + 80 ms timer between the hotkey and this call).
    HWND fg = GetForegroundWindow();
    DWORD fg_thread = fg ? GetWindowThreadProcessId(fg, nullptr) : 0;
    DWORD this_thread = GetCurrentThreadId();
    BOOL ok = FALSE;
    if (fg_thread && fg_thread != this_thread) {
        AttachThreadInput(fg_thread, this_thread, TRUE);
        ok = SetForegroundWindow(hwnd);
        AttachThreadInput(fg_thread, this_thread, FALSE);
    } else {
        ok = SetForegroundWindow(hwnd);
    }

    // Fallback: AttachThreadInput has been hardened against in newer Win11
    // builds. Synthesizing an Alt key press makes the OS believe the user
    // gave our process input, granting one foreground-change right. The Alt
    // press/release is harmless (no menu opens because no key follows).
    if (!ok) {
        INPUT inputs[2] = {};
        inputs[0].type       = INPUT_KEYBOARD;
        inputs[0].ki.wVk     = VK_MENU;
        inputs[1].type       = INPUT_KEYBOARD;
        inputs[1].ki.wVk     = VK_MENU;
        inputs[1].ki.dwFlags = KEYEVENTF_KEYUP;
        SendInput(2, inputs, sizeof(INPUT));
        SetForegroundWindow(hwnd);
    }

    // Belt-and-braces: ensure top of Z-order and focus on this window.
    BringWindowToTop(hwnd);
    SetFocus(hwnd);
#endif
}

WindowManager* WindowManager::m_instance = nullptr;

WindowManager& WindowManager::instance(QApplication* app) {
    if (!m_instance) {
        m_instance = new WindowManager(app);
        m_instance->m_window_id_count = 0;
    }
    return *m_instance;
}

WindowManager::WindowManager(QApplication* app, QObject* parent)
    : QObject(parent)
{
    this->m_app = app;

    QObject::connect(this, &WindowManager::signal_run_lookup_query, this, &WindowManager::run_lookup_query);
    QObject::connect(this, &WindowManager::signal_run_summary_query, this, &WindowManager::run_summary_query);
    QObject::connect(this, &WindowManager::signal_run_sutta_menu_action, this, &WindowManager::run_sutta_menu_action);
    QObject::connect(this, &WindowManager::signal_run_dppn_dictionary_query, this, &WindowManager::run_dppn_dictionary_query);
    QObject::connect(this, &WindowManager::signal_run_combined_dictionary_query, this, &WindowManager::run_combined_dictionary_query);
    QObject::connect(this, &WindowManager::signal_open_sutta_search_window, this, &WindowManager::open_sutta_search_window_with_query);
    QObject::connect(this, &WindowManager::signal_open_sutta_tab, this, &WindowManager::open_sutta_tab_in_window);
    QObject::connect(this, &WindowManager::signal_toggle_reading_mode, this, &WindowManager::toggle_reading_mode);
    QObject::connect(this, &WindowManager::signal_open_in_lookup_window, this, &WindowManager::open_in_lookup_window);
}

WindowManager::~WindowManager() {
    // Clean up all windows
    // FIXME: does this clean up work?
    while (!sutta_search_windows.isEmpty()) {
        auto w = sutta_search_windows.takeFirst();
        w->deleteLater();
    }

    while (!download_appdata_windows.isEmpty()) {
        auto w = download_appdata_windows.takeFirst();
        w->deleteLater();
    }

    while (!storage_recovery_windows.isEmpty()) {
        auto w = storage_recovery_windows.takeFirst();
        w->deleteLater();
    }

    while (!sutta_languages_windows.isEmpty()) {
        auto w = sutta_languages_windows.takeFirst();
        w->deleteLater();
    }

    while (!dictionaries_windows.isEmpty()) {
        auto w = dictionaries_windows.takeFirst();
        w->deleteLater();
    }

    while (!library_windows.isEmpty()) {
        auto w = library_windows.takeFirst();
        w->deleteLater();
    }

    while (!topic_index_windows.isEmpty()) {
        auto w = topic_index_windows.takeFirst();
        w->deleteLater();
    }

    while (!chanting_practice_windows.isEmpty()) {
        auto w = chanting_practice_windows.takeFirst();
        w->deleteLater();
    }

    while (!chanting_review_windows.isEmpty()) {
        auto w = chanting_review_windows.takeFirst();
        w->deleteLater();
    }
}

SuttaSearchWindow* WindowManager::create_sutta_search_window() {
    SuttaSearchWindow* w = new SuttaSearchWindow(this->m_app);
    sutta_search_windows.append(w);
    w->m_root->setProperty("window_id", QString("window_%1").arg(this->m_window_id_count));
    this->m_window_id_count++;
    return w;
}

void WindowManager::restore_last_session() {
    if (this->sutta_search_windows.length() == 0) {
        return;
    }

    // Check if restore is enabled by calling get_restore_last_session on the bridge via QML
    auto first_window = this->sutta_search_windows.first();
    if (!first_window->m_root) {
        return;
    }

    bool restore_enabled = false;
    QMetaObject::invokeMethod(first_window->m_root, "get_restore_last_session_setting",
        Q_RETURN_ARG(bool, restore_enabled));

    if (!restore_enabled) {
        return;
    }

    // Get last session data
    QString session_json;
    QMetaObject::invokeMethod(first_window->m_root, "get_last_session_json_from_bridge",
        Q_RETURN_ARG(QString, session_json));

    if (session_json.isEmpty() || session_json == "[]") {
        return;
    }

    QJsonDocument doc = QJsonDocument::fromJson(session_json.toUtf8());
    if (!doc.isArray()) {
        return;
    }

    QJsonArray windows = doc.array();
    for (int i = 0; i < windows.size(); i++) {
        QJsonObject window_obj = windows[i].toObject();
        QString window_json = QJsonDocument(window_obj).toJson(QJsonDocument::Compact);

        SuttaSearchWindow* target_window;
        if (i == 0) {
            // Restore into the existing first window
            target_window = first_window;
        } else {
            // Create additional windows for remaining session folders
            target_window = this->create_sutta_search_window();
        }

        if (target_window && target_window->m_root) {
            QMetaObject::invokeMethod(target_window->m_root, "restore_last_session",
                Q_ARG(QString, window_json));
        }
    }
}

DownloadAppdataWindow* WindowManager::create_download_appdata_window(
    const QVariantMap& initial_properties) {
    DownloadAppdataWindow* w = new DownloadAppdataWindow(this->m_app, initial_properties);
    download_appdata_windows.append(w);
    return w;
}

StorageRecoveryWindow* WindowManager::create_storage_recovery_window() {
    StorageRecoveryWindow* w = new StorageRecoveryWindow(this->m_app);
    storage_recovery_windows.append(w);
    return w;
}

SuttaLanguagesWindow* WindowManager::create_sutta_languages_window() {
    SuttaLanguagesWindow* w = new SuttaLanguagesWindow(this->m_app);
    sutta_languages_windows.append(w);
    return w;
}

DictionariesWindow* WindowManager::create_dictionaries_window() {
    // Reuse existing window if one exists
    for (auto w : this->dictionaries_windows) {
        if (w->m_root) {
            QMetaObject::invokeMethod(w->m_root, "show");
            QMetaObject::invokeMethod(w->m_root, "raise");
            QMetaObject::invokeMethod(w->m_root, "requestActivate");
            return w;
        }
    }
    DictionariesWindow* w = new DictionariesWindow(this->m_app);
    dictionaries_windows.append(w);
    return w;
}

LibraryWindow* WindowManager::create_library_window() {
    LibraryWindow* w = new LibraryWindow(this->m_app);
    library_windows.append(w);
    return w;
}

ReferenceSearchWindow* WindowManager::create_reference_search_window() {
    ReferenceSearchWindow* w = new ReferenceSearchWindow(this->m_app);
    reference_search_windows.append(w);
    return w;
}

TopicIndexWindow* WindowManager::create_topic_index_window() {
    TopicIndexWindow* w = new TopicIndexWindow(this->m_app);
    topic_index_windows.append(w);
    return w;
}

ChantingPracticeWindow* WindowManager::create_chanting_practice_window(const QString& window_id) {
    // Reuse existing ChantingPracticeWindow if one exists
    for (auto w : this->chanting_practice_windows) {
        if (w->m_root) {
            QMetaObject::invokeMethod(w->m_root, "show");
            QMetaObject::invokeMethod(w->m_root, "raise");
            QMetaObject::invokeMethod(w->m_root, "requestActivate");
            return w;
        }
    }

    ChantingPracticeWindow* w = new ChantingPracticeWindow(this->m_app, window_id);
    chanting_practice_windows.append(w);
    return w;
}

ChantingReviewWindow* WindowManager::create_chanting_review_window(const QString& window_id, const QString& section_uid) {
    ChantingReviewWindow* w = new ChantingReviewWindow(this->m_app, window_id, section_uid);
    chanting_review_windows.append(w);
    return w;
}

void WindowManager::run_lookup_query(const QString& query_text) {
    // Open a SuttaSearchWindow, set the query text, and run a Dictionary search
    // This is used by the browser extension to search for dictionary words
    // Reuse the same window for subsequent lookup queries

    const QString lookup_window_id = "window_lookup_query";
    SuttaSearchWindow* target_window = nullptr;

    // Find existing lookup query window
    for (auto w : this->sutta_search_windows) {
        QVariant prop = w->m_root->property("window_id");
        if (prop.isValid() && prop.toString() == lookup_window_id) {
            target_window = w;
            break;
        }
    }

    // Create a new window if none exists
    if (target_window == nullptr) {
        target_window = new SuttaSearchWindow(this->m_app);
        sutta_search_windows.append(target_window);
        target_window->m_root->setProperty("window_id", lookup_window_id);
    }

    if (target_window && target_window->m_root) {
        // Show, raise and activate (handles Windows foreground-stealing quirks
        // for the global-hotkey path).
        show_and_activate_window(target_window->m_root);

        // Call the QML run_lookup_query function which sets Dictionary mode and runs the search
        QMetaObject::invokeMethod(target_window->m_root, "run_lookup_query", Q_ARG(QString, query_text));
    }
}

void WindowManager::run_summary_query(const QString& window_id, const QString& query_text) {
    // NOTE: .isEmpty() returns true even when .length() > 0
    if (this->sutta_search_windows.length() == 0) {
        return;
    }

    SuttaSearchWindow* target_window = nullptr;
    for (auto w : this->sutta_search_windows) {
        QVariant prop = w->m_root->property("window_id");
        if (prop.isValid() && prop.toString() == window_id) {
            target_window = w;
            break;
        }
    }

    if (target_window == nullptr) {
        return;
    }

    QMetaObject::invokeMethod(target_window->m_root, "set_summary_query", Q_ARG(QString, query_text));
}

void WindowManager::run_dppn_dictionary_query(const QString& window_id, const QString& query) {
    SuttaSearchWindow* target_window = nullptr;
    for (auto w : this->sutta_search_windows) {
        QVariant prop = w->m_root->property("window_id");
        if (prop.isValid() && prop.toString() == window_id) {
            target_window = w;
            break;
        }
    }

    if (target_window == nullptr || target_window->m_root == nullptr) {
        QString msg = QString("run_dppn_dictionary_query: no window found for window_id: %1").arg(window_id);
        log_error_c(msg.toUtf8().constData());
        return;
    }

    QMetaObject::invokeMethod(target_window->m_root, "run_dppn_dictionary_query", Q_ARG(QString, query));
}

void WindowManager::run_combined_dictionary_query(const QString& window_id, const QString& query) {
    SuttaSearchWindow* target_window = nullptr;
    for (auto w : this->sutta_search_windows) {
        QVariant prop = w->m_root->property("window_id");
        if (prop.isValid() && prop.toString() == window_id) {
            target_window = w;
            break;
        }
    }

    if (target_window == nullptr || target_window->m_root == nullptr) {
        QString msg = QString("run_combined_dictionary_query: no window found for window_id: %1").arg(window_id);
        log_error_c(msg.toUtf8().constData());
        return;
    }

    QMetaObject::invokeMethod(target_window->m_root, "run_combined_dictionary_query", Q_ARG(QString, query));
}

void WindowManager::run_sutta_menu_action(const QString& window_id, const QString& action, const QString& query_text) {
    SuttaSearchWindow* target_window = nullptr;

    // Try to find the window by window_id
    for (auto w : this->sutta_search_windows) {
        QVariant prop = w->m_root->property("window_id");
        if (prop.isValid() && prop.toString() == window_id) {
            target_window = w;
            break;
        }
    }

    // Fallback: use the first available window, or create a new one
    if (target_window == nullptr) {
        if (this->sutta_search_windows.length() > 0) {
            target_window = this->sutta_search_windows.first();
        } else {
            target_window = this->create_sutta_search_window();
        }
    }

    if (target_window == nullptr || target_window->m_root == nullptr) {
        return;
    }

    // Show and raise the window
    QMetaObject::invokeMethod(target_window->m_root, "show");
    QMetaObject::invokeMethod(target_window->m_root, "raise");
    QMetaObject::invokeMethod(target_window->m_root, "requestActivate");

    QMetaObject::invokeMethod(target_window->m_root, "run_sutta_menu_action", Q_ARG(QString, action), Q_ARG(QString, query_text));
}

void WindowManager::open_sutta_search_window_with_query(const QString& show_result_data_json) {
    SuttaSearchWindow* w = this->create_sutta_search_window();

    // If result data JSON is provided, show the sutta directly
    if (!show_result_data_json.isEmpty() && w && w->m_root) {
        QMetaObject::invokeMethod(w->m_root, "show_result_in_html_view_with_json",
            Q_ARG(QString, show_result_data_json),
            Q_ARG(QVariant, QVariant(false)));  // Don't create new tab in fresh window
    }
}

void WindowManager::open_sutta_tab_in_window(const QString& window_id, const QString& show_result_data_json) {
    // Find the window with matching window_id
    SuttaSearchWindow* target_window = nullptr;

    if (this->sutta_search_windows.length() == 0) {
        return;
    }

    if (window_id.isEmpty()) {
        // Fall back to last window if no window_id provided
        target_window = this->sutta_search_windows.last();
    } else {
        // Find the window with matching window_id
        for (auto w : this->sutta_search_windows) {
            QVariant prop = w->m_root->property("window_id");
            if (prop.isValid() && prop.toString() == window_id) {
                target_window = w;
                break;
            }
        }
    }

    if (target_window && target_window->m_root) {
        // Show and raise the window
        QMetaObject::invokeMethod(target_window->m_root, "show");
        QMetaObject::invokeMethod(target_window->m_root, "raise");

        // Show the sutta in a new tab
        QMetaObject::invokeMethod(target_window->m_root, "show_result_in_html_view_with_json",
            Q_ARG(QString, show_result_data_json),
            Q_ARG(QVariant, QVariant(true)));  // Pass true to create a new tab
    }
}

void WindowManager::show_chapter_in_sutta_window(const QString& window_id, const QString& result_data_json) {
    // If window_id is empty, fall back to the last window (for backwards compatibility)
    // Otherwise, find the specific window by window_id
    SuttaSearchWindow* target_window = nullptr;

    if (this->sutta_search_windows.length() == 0) {
        return;
    }

    if (window_id.isEmpty()) {
        // Fall back to last window if no window_id provided
        target_window = this->sutta_search_windows.last();
    } else {
        // Find the window with matching window_id
        for (auto w : this->sutta_search_windows) {
            QVariant prop = w->m_root->property("window_id");
            if (prop.isValid() && prop.toString() == window_id) {
                target_window = w;
                break;
            }
        }
    }

    if (target_window && target_window->m_root) {
        // Show and raise the window
        QMetaObject::invokeMethod(target_window->m_root, "show");
        QMetaObject::invokeMethod(target_window->m_root, "raise");

        // Show the chapter in the HTML view (replace current tab, don't create new)
        QMetaObject::invokeMethod(target_window->m_root, "show_result_in_html_view_with_json",
            Q_ARG(QString, result_data_json),
            Q_ARG(QVariant, QVariant(false)));
    }
}

void WindowManager::show_sutta_from_reference_search(const QString& window_id, const QString& result_data_json) {
    // If window_id is empty, fall back to the last window (for backwards compatibility)
    // Otherwise, find the specific window by window_id
    SuttaSearchWindow* target_window = nullptr;

    if (this->sutta_search_windows.length() == 0) {
        return;
    }

    if (window_id.isEmpty()) {
        // Fall back to last window if no window_id provided
        target_window = this->sutta_search_windows.last();
    } else {
        // Find the window with matching window_id
        for (auto w : this->sutta_search_windows) {
            QVariant prop = w->m_root->property("window_id");
            if (prop.isValid() && prop.toString() == window_id) {
                target_window = w;
                break;
            }
        }
    }

    if (target_window && target_window->m_root) {
        // Show and raise the window
        QMetaObject::invokeMethod(target_window->m_root, "show");
        QMetaObject::invokeMethod(target_window->m_root, "raise");

        // Show the sutta in the HTML view (create a new tab)
        QMetaObject::invokeMethod(target_window->m_root, "show_result_in_html_view_with_json",
            Q_ARG(QString, result_data_json),
            Q_ARG(QVariant, QVariant(true)));  // Pass true to create a new tab
    }
}

void WindowManager::toggle_reading_mode(const QString& window_id, bool is_active) {
    if (this->sutta_search_windows.length() == 0) {
        return;
    }

    SuttaSearchWindow* target_window = nullptr;
    for (auto w : this->sutta_search_windows) {
        QVariant prop = w->m_root->property("window_id");
        if (prop.isValid() && prop.toString() == window_id) {
            target_window = w;
            break;
        }
    }

    if (target_window == nullptr) {
        return;
    }

    QMetaObject::invokeMethod(target_window->m_root, "toggle_search_ui_visibility", Q_ARG(bool, !is_active));
}

void WindowManager::open_in_lookup_window(const QString& result_data_json) {
    // Open a sutta or dictionary result in the dedicated lookup window
    // Reuses the same window for subsequent requests, adds results as new tabs

    const QString lookup_window_id = "window_lookup_query";
    SuttaSearchWindow* target_window = nullptr;

    // Find existing lookup query window
    for (auto w : this->sutta_search_windows) {
        QVariant prop = w->m_root->property("window_id");
        if (prop.isValid() && prop.toString() == lookup_window_id) {
            target_window = w;
            break;
        }
    }

    // Create a new window if none exists
    if (target_window == nullptr) {
        target_window = new SuttaSearchWindow(this->m_app);
        sutta_search_windows.append(target_window);
        target_window->m_root->setProperty("window_id", lookup_window_id);
    }

    if (target_window && target_window->m_root) {
        // Show, raise and activate (handles Windows foreground-stealing quirks).
        show_and_activate_window(target_window->m_root);

        // Show the result in a new tab in the results group
        QMetaObject::invokeMethod(target_window->m_root, "show_result_in_html_view_with_json",
            Q_ARG(QString, result_data_json),
            Q_ARG(QVariant, QVariant(true)));  // Pass true to create a new tab
    }
}
