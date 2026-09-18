#ifndef CHANTING_REVIEW_WINDOW_H
#define CHANTING_REVIEW_WINDOW_H

#include <QObject>
#include <QString>
#include <QApplication>
#include <QQmlApplicationEngine>

class ChantingReviewWindow : public QObject {
    Q_OBJECT

public:
    explicit ChantingReviewWindow(QApplication* app, const QString& window_id, const QString& section_uid, bool auto_start_recording = false, QObject* parent = nullptr);
    ~ChantingReviewWindow();

    QApplication* m_app;
    QObject* m_root;
    QQmlApplicationEngine *m_engine;
    QString m_window_id;
    QString m_section_uid;

    void apply_window_properties(const QString& window_id, const QString& section_uid);

    /// Ask the QML window to add a new recording panel and start recording in it.
    void start_quick_recording();

private:
    void setup_qml();
};

#endif
