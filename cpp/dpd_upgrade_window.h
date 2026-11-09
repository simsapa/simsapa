#ifndef DPD_UPGRADE_WINDOW_H
#define DPD_UPGRADE_WINDOW_H

#include <QObject>
#include <QApplication>
#include <QQmlApplicationEngine>

// The DPD upgrade progress window, opened by gui.cpp in place of the main app
// when the auto_start_dpd_upgrade.txt marker holds a release tag. Like
// DownloadAppdataWindow it lives in its own app.exec(), which the app exits
// when the window closes. See docs/dpd-upgrades.md.
class DpdUpgradeWindow : public QObject {
    Q_OBJECT

public:
    explicit DpdUpgradeWindow(QApplication* app, QObject* parent = nullptr);
    ~DpdUpgradeWindow();

    QApplication* m_app;
    QObject* m_root;
    QQmlApplicationEngine *m_engine;

private:
    void setup_qml();
};

#endif
