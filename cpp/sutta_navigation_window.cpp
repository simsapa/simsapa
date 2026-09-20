#include "sutta_navigation_window.h"

#include <QUrl>

SuttaNavigationWindow::SuttaNavigationWindow(QApplication* app, QObject* parent)
    : QObject(parent)
{
    this->m_app = app;
    setup_qml();
}

void SuttaNavigationWindow::setup_qml() {
    const QUrl view_qml(QStringLiteral("qrc:/qt/qml/com/profoundlabs/simsapa/assets/qml/SuttaNavigationWindow.qml"));
    m_engine = new QQmlApplicationEngine(view_qml, this);
    // An engine load failure leaves rootObjects() empty; constFirst() on an empty
    // list is undefined behaviour. A null m_root is a reachable state that
    // WindowManager evicts from its list rather than reusing.
    m_root = m_engine->rootObjects().isEmpty() ? nullptr : m_engine->rootObjects().constFirst();
}

/// Never reached in normal operation -- this window is hidden on close, not
/// destroyed, and nothing calls on_window_closed() for it. The destructor exists
/// for the eviction path in reuse_or_evict(), which deleteLater()s a wrapper
/// whose engine load failed.
SuttaNavigationWindow::~SuttaNavigationWindow() {
    delete m_engine;
}
