#ifndef SEARCH_INDEX_REBUILD_WINDOW_H
#define SEARCH_INDEX_REBUILD_WINDOW_H

#include <QObject>
#include <QApplication>
#include <QQmlApplicationEngine>

class SearchIndexRebuildWindow : public QObject {
    Q_OBJECT

public:
    explicit SearchIndexRebuildWindow(QApplication* app, QObject* parent = nullptr);
    ~SearchIndexRebuildWindow();

    QApplication* m_app;
    QObject* m_root;
    QQmlApplicationEngine *m_engine;

private:
    void setup_qml();
};

#endif
