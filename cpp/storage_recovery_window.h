#ifndef STORAGE_RECOVERY_WINDOW_H
#define STORAGE_RECOVERY_WINDOW_H

#include <QObject>
#include <QApplication>
#include <QQmlApplicationEngine>

// Host for StorageRecoveryWindow.qml — the startup flow shown when the recorded
// storage location is unreachable or holds no installation.
//
// The QML resolves the flow itself (scan, adoption, Try Again, the restart
// notices). Only the two outcomes that need the download flow come back here,
// because DownloadAppdataWindow is created on the C++ side:
//
//   download_here(path)  the user picked a location in the recovery dialog, so
//                        the download window must NOT ask for one again
//   declined()           an ordinary first-time install, storage dialog and all
//
// See docs/relocated-storage-recovery.md.
class StorageRecoveryWindow : public QObject {
    Q_OBJECT

public:
    explicit StorageRecoveryWindow(QApplication* app, QObject* parent = nullptr);
    ~StorageRecoveryWindow();

    QApplication* m_app;
    QObject* m_root;
    QQmlApplicationEngine* m_engine;

private slots:
    void handle_download_here(const QString& path);
    void handle_declined();

private:
    void setup_qml();
    void run_first_time_install(bool skip_storage_dialog);
};

#endif
