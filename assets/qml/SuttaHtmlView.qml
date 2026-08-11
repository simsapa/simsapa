import QtQuick

Loader {
    id: loader
    required property string window_id
    required property bool is_dark
    required property bool is_reading_mode
    required property string item_key

    property bool should_be_visible: true

    // Passing on tab_data properties as json to avoid the UI reacting to one
    // property (e.g. item_uid) while the other has not yet been set (e.g.
    // table_name).
    required property string data_json
    // NOTE: data_json properties:
    // let data = {
    //     item_uid: tab_data.item_uid,
    //     table_name: tab_data.table_name,
    //     sutta_ref: tab_data.sutta_ref,
    //     sutta_title: tab_data.sutta_title,
    //     anchor: tab_data.anchor,
    // };

    function get_data_value(key: string): string {
        let data = JSON.parse(loader.data_json);
        return data[key];
    }

    function set_data_value(key: string, value: string): string {
        let data = JSON.parse(loader.data_json);
        data[key] = value;
        loader.data_json = JSON.stringify(data);
    }

    function active_focus() {
        loader.item.web.forceActiveFocus(); // qmllint disable missing-property
    }

    function show_transient_message(msg) {
        loader.item.show_transient_message(msg); // qmllint disable missing-property
    }

    // Force the embedded webview to re-send its size to its rendering engine
    // after the surrounding layout gave it back some height. No-op on desktop.
    function nudge_webview_geometry() {
        if (loader.item) {
            loader.item.nudge_webview_geometry(); // qmllint disable missing-property
        }
    }

    // Qt's height for the embedded webview at call time, for the VIEWPORT-NUDGE
    // diagnostic's `qt_h0` field only — the surrounding layout may not have
    // settled yet. The comparable value is reported later by the mobile view.
    function webview_height(): real {
        return loader.item ? loader.item.webview_height() : 0; // qmllint disable missing-property
    }

    function show_find_bar() {
        loader.item.show_find_bar(); // qmllint disable missing-property
    }

    function find_next() {
        loader.item.find_next(); // qmllint disable missing-property
    }

    function find_previous() {
        loader.item.find_previous(); // qmllint disable missing-property
    }

    // Scroll functions
    function scroll_small_up() {
        loader.item.scroll_small_up(); // qmllint disable missing-property
    }

    function scroll_small_down() {
        loader.item.scroll_small_down(); // qmllint disable missing-property
    }

    function scroll_half_page_up() {
        loader.item.scroll_half_page_up(); // qmllint disable missing-property
    }

    function scroll_half_page_down() {
        loader.item.scroll_half_page_down(); // qmllint disable missing-property
    }

    function scroll_page_up() {
        loader.item.scroll_page_up(); // qmllint disable missing-property
    }

    function scroll_page_down() {
        loader.item.scroll_page_down(); // qmllint disable missing-property
    }

    function scroll_to_top() {
        loader.item.scroll_to_top(); // qmllint disable missing-property
    }

    function scroll_to_bottom() {
        loader.item.scroll_to_bottom(); // qmllint disable missing-property
    }

    signal page_loaded()

    /* signal loadingChanged(var loadRequest) */

    source: {
        if (Qt.platform.os === "android" || Qt.platform.os === "ios") {
            return "SuttaHtmlView_Mobile.qml";
        } else {
            return "SuttaHtmlView_Desktop.qml";
        }
    }

    // Desktop: load the WebEngineView through the incubator instead of
    // synchronously, so creating a tab's webview doesn't block the frame it
    // was requested in (the first WebEngineView carries the Chromium
    // bring-up on the GUI thread). Callers must tolerate a briefly-null
    // `item`; all programmatic `.item` accesses are user-driven or run from
    // page_loaded, by which time the item exists. Mobile keeps the
    // synchronous default — the native WebView is cheap and the mobile
    // visibility-management code
    // (docs/mobile-webview-visibility-management.md) was written against
    // synchronous creation order. See docs/startup-sequence-and-caches.md
    // §"First paint and the pre-exec stall".
    asynchronous: Qt.platform.os !== "android" && Qt.platform.os !== "ios"

    onLoaded: {
        loader.item.window_id = Qt.binding(() => window_id);
        loader.item.is_dark = Qt.binding(() => is_dark);
        loader.item.is_reading_mode = Qt.binding(() => is_reading_mode);
        loader.item.data_json = Qt.binding(() => data_json);
        loader.item.visible = Qt.binding(() => loader.should_be_visible && loader.visible);
        loader.item.page_loaded.connect(function() { loader.page_loaded(); }); // qmllint disable missing-property
    }

}

