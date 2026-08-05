#include "download_appdata_window.h"

#include <QSysInfo>
#include <QUrl>

DownloadAppdataWindow::DownloadAppdataWindow(QApplication* app,
                                             const QVariantMap& initial_properties,
                                             QObject* parent)
    : QObject(parent)
{
    this->m_app = app;
    setup_qml(initial_properties);
}

void DownloadAppdataWindow::setup_qml(const QVariantMap& initial_properties) {
    QUrl view_qml;
    view_qml = QUrl(QStringLiteral("qrc:/qt/qml/com/profoundlabs/simsapa/assets/qml/DownloadAppdataWindow.qml"));

    // Constructed empty and loaded explicitly, rather than with the URL-taking
    // constructor, so the initial properties can be set first: the URL
    // constructor loads immediately, which would run Component.onCompleted
    // before anything could be applied.
    m_engine = new QQmlApplicationEngine(this);
    if (!initial_properties.isEmpty()) {
        m_engine->setInitialProperties(initial_properties);
    }
    m_engine->load(view_qml);

    m_root = m_engine->rootObjects().isEmpty() ? nullptr : m_engine->rootObjects().constFirst();
}

DownloadAppdataWindow::~DownloadAppdataWindow() {
    delete m_engine;
}

