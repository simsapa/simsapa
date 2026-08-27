import QtQuick

Loader {
    id: loader
    required property string window_id
    required property string word_uid
    required property bool is_dark

    /* signal loadingChanged(var loadRequest) */

    source: {
        if (Qt.platform.os === "android" || Qt.platform.os === "ios") {
            return "DictionaryHtmlView_Mobile.qml";
        } else {
            return "DictionaryHtmlView_Desktop.qml";
        }
    }

    // Desktop: asynchronous for the same reason as SuttaHtmlView — the
    // DictionaryTab (and its webview) is instantiated eagerly during the
    // SuttaSearchWindow QML parse, and a synchronous WebEngineView creation
    // there runs before the window can paint its first frame. See
    // docs/startup-sequence-and-caches.md §"First paint and the pre-exec
    // stall".
    asynchronous: Qt.platform.os !== "android" && Qt.platform.os !== "ios"

    onLoaded: {
        loader.item.window_id = Qt.binding(() => window_id);
        loader.item.word_uid = Qt.binding(() => word_uid);
        loader.item.is_dark = Qt.binding(() => is_dark);
        loader.item.visible = Qt.binding(() => loader.visible);
        /* loader.item.loadingChanged.connect(loader.loadingChanged); */
    }

}
