#include "dictionaries_window.h"

#include <QUrl>

DictionariesWindow::DictionariesWindow(QApplication* app, QObject* parent)
    : QObject(parent)
{
    this->m_app = app;
    setup_qml();
}

void DictionariesWindow::setup_qml() {
    const QUrl view_qml(QStringLiteral("qrc:/qt/qml/com/profoundlabs/simsapa/assets/qml/DictionariesWindow.qml"));
    m_engine = new QQmlApplicationEngine(view_qml, this);
    // An engine load failure leaves rootObjects() empty; constFirst() on an empty
    // list is undefined behaviour. A null m_root is a reachable state that
    // WindowManager evicts from its list rather than reusing.
    m_root = m_engine->rootObjects().isEmpty() ? nullptr : m_engine->rootObjects().constFirst();
}

DictionariesWindow::~DictionariesWindow() {
    delete m_engine;
}
