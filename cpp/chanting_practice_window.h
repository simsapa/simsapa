#ifndef CHANTING_PRACTICE_WINDOW_H
#define CHANTING_PRACTICE_WINDOW_H

#include <QObject>
#include <QApplication>
#include <QQmlApplicationEngine>

class ChantingPracticeWindow : public QObject {
    Q_OBJECT

public:
    explicit ChantingPracticeWindow(QApplication* app, const QString& window_id, QObject* parent = nullptr);
    ~ChantingPracticeWindow();

    QApplication* m_app;
    QObject* m_root;
    QQmlApplicationEngine *m_engine;
    QString m_window_id;

    void apply_window_properties(const QString& window_id);

private:
    void setup_qml();
};

#endif
