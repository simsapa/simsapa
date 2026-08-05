#include "storage_recovery_window.h"
#include "download_appdata_window.h"
#include "window_manager.h"

#include <QUrl>

extern "C" void log_info_c(const char* msg);
extern "C" void log_error_c(const char* msg);

StorageRecoveryWindow::StorageRecoveryWindow(QApplication* app, QObject* parent)
    : QObject(parent)
{
    this->m_app = app;
    setup_qml();
}

void StorageRecoveryWindow::setup_qml() {
    QUrl view_qml(QStringLiteral("qrc:/qt/qml/com/profoundlabs/simsapa/assets/qml/StorageRecoveryWindow.qml"));
    m_engine = new QQmlApplicationEngine(view_qml, this);

    // Never constFirst() on a possibly-empty list: a QML load failure leaves
    // rootObjects() empty and constFirst() is undefined behaviour there. The
    // null is the caller's signal to fall back to the ordinary first-run window
    // — this window is the app's only one at that point, so entering app.exec()
    // with nothing on screen would hang with a blank display and no way out
    // (no window can close, so quitOnLastWindowClosed never fires).
    m_root = m_engine->rootObjects().isEmpty() ? nullptr : m_engine->rootObjects().constFirst();

    if (m_root == nullptr) {
        log_error_c("StorageRecoveryWindow: the QML root object is null");
        return;
    }

    // The QML posts its first scan with Qt.callLater, so these connections are
    // in place before any handoff signal can be emitted. A signal emitted during
    // the engine load would be delivered to nobody.
    QObject::connect(m_root, SIGNAL(download_here(QString)),
                     this, SLOT(handle_download_here(QString)));
    QObject::connect(m_root, SIGNAL(declined()),
                     this, SLOT(handle_declined()));
}

void StorageRecoveryWindow::handle_download_here(const QString& path) {
    log_info_c(QString("StorageRecoveryWindow: downloading to the selected location: %1")
                   .arg(path)
                   .toUtf8()
                   .constData());
    run_first_time_install(true);
}

void StorageRecoveryWindow::handle_declined() {
    log_info_c("StorageRecoveryWindow: setting up a new database");
    run_first_time_install(false);
}

void StorageRecoveryWindow::run_first_time_install(bool skip_storage_dialog) {
    QVariantMap initial_properties;

    if (skip_storage_dialog) {
        // Suppress DownloadAppdataWindow's own storage dialog: the user has just
        // chosen a location in the recovery dialog, and asking again is asking
        // twice.
        //
        // Set ONLY on the group-2 handoff. The "reachable_empty, nothing found"
        // fall-through also reaches the download flow with a location already
        // recorded, but there the choice was made in a previous session before a
        // download that then failed — the location itself is the prime suspect,
        // so the dialog must still open.
        initial_properties.insert(QStringLiteral("skip_storage_dialog"), true);
    } else {
        // The other outcome — "set up a new database" (Set Up Again / Create New
        // Location / decline). A pending upgrade marker must not hijack it: the
        // download would auto-start, skipping the storage dialog the user needs,
        // and land at whatever location resolves on its own — in the unreachable
        // state the internal fallback, which is neither what the marker recorded
        // nor what the user chose.
        //
        // Passed as an INITIAL property so it applies before
        // Component.onCompleted: should_auto_start_download() deletes the marker
        // as a side effect of reporting it, so a later flag would come too late
        // to stop the consumption. Leaving the marker in place means an
        // interrupted upgrade download can still resume once the user's storage
        // question is settled.
        initial_properties.insert(QStringLiteral("skip_auto_start_download"), true);
    }

    DownloadAppdataWindow* w =
        WindowManager::instance(this->m_app).create_download_appdata_window(initial_properties);

    if (w == nullptr || w->m_root == nullptr) {
        log_error_c("StorageRecoveryWindow: the download window could not be created");
        return;
    }

    // Hidden only after the download window exists, so the application is never
    // momentarily without a window (which would quit the event loop).
    if (m_root != nullptr) {
        m_root->setProperty("visible", false);
    }
}

StorageRecoveryWindow::~StorageRecoveryWindow() {
    delete m_engine;
}
