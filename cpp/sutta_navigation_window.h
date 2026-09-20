#ifndef SUTTA_NAVIGATION_WINDOW_H
#define SUTTA_NAVIGATION_WINDOW_H

#include <QObject>
#include <QApplication>
#include <QQmlApplicationEngine>

/// The corpus navigation tree.
///
/// A third lifecycle family: one instance, but **hidden** on close rather than
/// destroyed. The QML `onClosing` accepts the close and does not notify the
/// WindowManager, so this wrapper is never torn down and the breadcrumb path,
/// expansion state, filter text and scroll position survive a close/reopen with
/// no save/restore code at all. The reuse predicate is `m_root != nullptr`, as
/// for the single-instance family -- not `visible`, which would construct a
/// second instance every time the user reopened it.
///
/// See docs/window-lifecycle-and-reuse.md.
class SuttaNavigationWindow : public QObject {
    Q_OBJECT

public:
    explicit SuttaNavigationWindow(QApplication* app, QObject* parent = nullptr);
    ~SuttaNavigationWindow();

    QApplication* m_app;
    QObject* m_root;
    QQmlApplicationEngine *m_engine;

private:
    void setup_qml();
};

#endif
