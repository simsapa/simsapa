import QtQuick

/* Works around the QtWebEngine stale-frame bug on Linux: while the app window
 * is inactive/occluded, Chromium stops producing compositor frames, and on
 * re-activation the scene graph can be left holding an invalid texture — the
 * webview shows solid black until something forces Chromium to submit a fresh
 * frame (e.g. a CSS hover state repaint, or resizing the window; see
 * QTBUG-54127). A JS-only repaint nudge proved insufficient, so this forces a
 * 1px resize jiggle of the WebEngineView shortly after the window becomes
 * active again — a resize makes Chromium re-render unconditionally.
 *
 * Usage (inside a component that contains a WebEngineView):
 *
 *     WebEngineRepaintNudge { web: web }
 */
Item {
    id: root

    // The WebEngineView to nudge. Must be anchored (anchors.fill), so the
    // jiggle can be applied via anchors.bottomMargin.
    required property var web

    Logger { id: logger }

    Connections {
        target: root.Window.window
        function onActiveChanged() {
            if (root.Window.window && root.Window.window.active) {
                logger.info("WebEngineRepaintNudge: window re-activated, resize-nudging webview");
                nudge_timer.restart();
            }
        }
    }

    // Deferred so the jiggle lands after the window re-activation churn.
    Timer {
        id: nudge_timer
        interval: 50
        onTriggered: {
            root.web.anchors.bottomMargin = 1;
            restore_timer.restart();
        }
    }

    Timer {
        id: restore_timer
        interval: 50
        onTriggered: {
            root.web.anchors.bottomMargin = 0;
        }
    }
}
