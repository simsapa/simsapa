#include "chanting_practice_window.h"

#include <QUrl>

ChantingPracticeWindow::ChantingPracticeWindow(QApplication* app, const QString& window_id, QObject* parent)
    : QObject(parent)
{
    this->m_app = app;
    this->m_window_id = window_id;
    setup_qml();
}

void ChantingPracticeWindow::setup_qml() {
    const QUrl view_qml(QStringLiteral("qrc:/qt/qml/com/profoundlabs/simsapa/assets/qml/ChantingPracticeWindow.qml"));
    m_engine = new QQmlApplicationEngine(view_qml, this);
    // An engine load failure leaves rootObjects() empty; constFirst() on an empty
    // list is undefined behaviour. A null m_root is a reachable state that
    // WindowManager evicts from its list rather than reusing.
    m_root = m_engine->rootObjects().isEmpty() ? nullptr : m_engine->rootObjects().constFirst();
    apply_window_properties(m_window_id);
}

/// Push the constructor parameters onto the QML root. Also called by
/// WindowManager when an existing instance is reused for a new open, so the
/// window does not keep showing the previous window_id.
void ChantingPracticeWindow::apply_window_properties(const QString& window_id) {
    this->m_window_id = window_id;
    if (!m_root) return;
    m_root->setProperty("window_id", m_window_id);
}

ChantingPracticeWindow::~ChantingPracticeWindow() {
    delete m_engine;
}
