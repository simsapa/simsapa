#include "sutta_search_window.h"

#include <QSysInfo>
#include <QUrl>

extern "C" void log_info_c(const char* msg);

SuttaSearchWindow::SuttaSearchWindow(QApplication* app, QObject* parent)
    : QObject(parent)
{
    this->m_app = app;
    setup_qml();
}

void SuttaSearchWindow::setup_qml() {
    QUrl view_qml;
    view_qml = QUrl(QStringLiteral("qrc:/qt/qml/com/profoundlabs/simsapa/assets/qml/SuttaSearchWindow.qml"));
    log_info_c("STARTUP-TRACE: engine.load() start");
    m_engine = new QQmlApplicationEngine(view_qml, this);
    log_info_c("STARTUP-TRACE: engine.load() end");
    m_root = m_engine->rootObjects().constFirst();
}

SuttaSearchWindow::~SuttaSearchWindow() {
    delete m_engine;
}

