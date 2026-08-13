#include "chanting_review_window.h"

#include <QUrl>
#include <QQmlContext>

ChantingReviewWindow::ChantingReviewWindow(QApplication* app, const QString& window_id, const QString& section_uid, QObject* parent)
    : QObject(parent)
{
    this->m_app = app;
    this->m_window_id = window_id;
    this->m_section_uid = section_uid;
    setup_qml();
}

void ChantingReviewWindow::setup_qml() {
    const QUrl view_qml(QStringLiteral("qrc:/qt/qml/com/profoundlabs/simsapa/assets/qml/ChantingPracticeReviewWindow.qml"));
    m_engine = new QQmlApplicationEngine(this);
    m_engine->load(view_qml);
    // An engine load failure leaves rootObjects() empty; constFirst() on an empty
    // list is undefined behaviour. A null m_root is a reachable state that
    // WindowManager evicts from its list rather than reusing.
    m_root = m_engine->rootObjects().isEmpty() ? nullptr : m_engine->rootObjects().constFirst();
    apply_window_properties(m_window_id, m_section_uid);
}

/// Push the constructor parameters onto the QML root. Also called by
/// WindowManager when an existing instance is reused for a new open.
///
/// Setting current_section_uid *is* the re-init: ChantingPracticeReviewWindow.qml
/// has an onCurrent_section_uidChanged handler that reloads the section whenever
/// the uid changes and differs from loaded_section_uid. Do not add a separate
/// invokeMethod re-init call here -- it would load the section twice. Reopening
/// the *same* section leaves the uid unchanged and fires no reload, which is
/// correct: the content on screen is already the right one.
void ChantingReviewWindow::apply_window_properties(const QString& window_id, const QString& section_uid) {
    this->m_window_id = window_id;
    this->m_section_uid = section_uid;
    if (!m_root) return;
    m_root->setProperty("window_id", m_window_id);
    m_root->setProperty("current_section_uid", m_section_uid);
}

ChantingReviewWindow::~ChantingReviewWindow() {
    delete m_engine;
}
