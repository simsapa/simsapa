#include "dpd_upgrade_window.h"

#include <QUrl>

DpdUpgradeWindow::DpdUpgradeWindow(QApplication* app, QObject* parent)
    : QObject(parent)
{
    this->m_app = app;
    setup_qml();
}

void DpdUpgradeWindow::setup_qml() {
    const QUrl view_qml(QStringLiteral("qrc:/qt/qml/com/profoundlabs/simsapa/assets/qml/DpdUpgradeWindow.qml"));
    m_engine = new QQmlApplicationEngine(this);
    m_engine->load(view_qml);
    m_root = m_engine->rootObjects().isEmpty() ? nullptr : m_engine->rootObjects().constFirst();
}

DpdUpgradeWindow::~DpdUpgradeWindow() {
    delete m_engine;
}
