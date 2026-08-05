#ifndef DOWNLOAD_APPDATA_WINDOW_H
#define DOWNLOAD_APPDATA_WINDOW_H

#include <QObject>
#include <QApplication>
#include <QQmlApplicationEngine>
#include <QVariantMap>

class DownloadAppdataWindow : public QObject {
    Q_OBJECT

public:
    // `initial_properties` are applied to the QML root BEFORE its
    // Component.onCompleted runs. The storage recovery flow depends on that
    // ordering: `skip_auto_start_download` has to be in place before
    // `should_auto_start_download()` deletes the upgrade marker as a side
    // effect of reporting it. See docs/relocated-storage-recovery.md.
    explicit DownloadAppdataWindow(QApplication* app,
                                   const QVariantMap& initial_properties = QVariantMap(),
                                   QObject* parent = nullptr);
    ~DownloadAppdataWindow();

    QApplication* m_app;
    QObject* m_root;
    QQmlApplicationEngine *m_engine;

private:
    void setup_qml(const QVariantMap& initial_properties);
};

#endif
