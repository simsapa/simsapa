import QtQuick
import QtWebEngine

import com.profoundlabs.simsapa

Item {
    id: root

    Logger { id: logger }

    anchors.fill: parent

    property string window_id
    property bool is_dark
    property bool is_reading_mode

    property string data_json

    property string item_uid
    property string table_name
    property string sutta_ref
    property string sutta_title
    property string anchor

    property alias web: web

    signal page_loaded()

    // Gate page_loaded emission on having seen a LoadStartedStatus since the
    // last emit, to filter out spurious LoadSucceededStatus events fired for
    // a target URL before its navigation actually begins (the previous page's
    // DOM is still live at that moment). Consumers of page_loaded
    // (setSearchTerm, bookmark scroll, ...) must not run against the stale
    // DOM.
    property bool load_in_progress: false

    Timer {
        id: scroll_timer
        interval: 300
        repeat: false
        onTriggered: root.scroll_to_anchor()
    }

    // No-op counterpart of the mobile view's geometry nudge, so callers do not
    // have to branch on platform. The bug it works around is a native-WebView
    // resize propagation failure; QtWebEngine has never shown it here.
    function nudge_webview_geometry() {
    }

    function webview_height(): real {
        return web.height;
    }

    function set_properties_from_data_json() {
        if (!root.data_json || root.data_json.length === 0) {
            return;
        }
        try {
            let data = JSON.parse(root.data_json);
            root.item_uid = data.item_uid || "";
            root.table_name = data.table_name || "";
            root.sutta_ref = data.sutta_ref || "";
            root.sutta_title = data.sutta_title || "";
            root.anchor = data.anchor || "";
        } catch (e) {
            logger.error("Failed to parse data_json: " + e + " data_json: " + root.data_json);
        }
    }

    function show_transient_message(msg: string) {
        let js = `var msg = \`${msg}\`; document.SSP.show_transient_message(msg, "transient-messages-top");`;
        web.runJavaScript(js);
    }

    function show_find_bar() {
        web.forceActiveFocus();
        web.runJavaScript(`document.SSP.find.show();`);
    }

    function find_next() {
        web.runJavaScript(`document.SSP.find.nextMatch();`);
    }

    function find_previous() {
        web.runJavaScript(`document.SSP.find.previousMatch();`);
    }

    // Scroll functions
    function scroll_small_up() {
        web.runJavaScript(`document.SSP.scroll.scrollSmallUp();`);
    }

    function scroll_small_down() {
        web.runJavaScript(`document.SSP.scroll.scrollSmallDown();`);
    }

    function scroll_half_page_up() {
        web.runJavaScript(`document.SSP.scroll.scrollHalfPageUp();`);
    }

    function scroll_half_page_down() {
        web.runJavaScript(`document.SSP.scroll.scrollHalfPageDown();`);
    }

    function scroll_page_up() {
        web.runJavaScript(`document.SSP.scroll.scrollPageUp();`);
    }

    function scroll_page_down() {
        web.runJavaScript(`document.SSP.scroll.scrollPageDown();`);
    }

    function scroll_to_top() {
        web.runJavaScript(`document.SSP.scroll.scrollToTop();`);
    }

    function scroll_to_bottom() {
        web.runJavaScript(`document.SSP.scroll.scrollToBottom();`);
    }

    function load_sutta_uid(uid) {
        if (uid == "Sutta") {
            // Initial blank page
            uid = "";
        }

        // For empty UID, use loadHtml to avoid 404 from API endpoint
        if (uid === "") {
            var html = SuttaBridge.get_sutta_html(root.window_id, "");
            web.loadHtml(html);
            return;
        }

        const api_url = SuttaBridge.get_api_url();
        const enc_uid = uid.split("/").map(encodeURIComponent).join("/");
        let url = `${api_url}/get_sutta_html_by_uid/${root.window_id}/${enc_uid}/`;
        if (root.anchor && root.anchor.length > 0) {
            // Pass anchor as query parameter so server renders reference elements
            // Also append as URL fragment for browser scrolling
            let anchor_value = root.anchor.startsWith('#') ? root.anchor.substring(1) : root.anchor;
            url = `${url}?anchor=${encodeURIComponent(anchor_value)}#${anchor_value}`;
        }
        web.url = url;
    }

    function load_word_uid(uid) {
        if (uid == "Word") {
            // Initial blank page
            uid = "";
        }

        if (root.table_name === "dpd_headwords") {
            // Results from DPD Lookup are in the form of
            // "item_uid": "25671/dpd", "table_name": "dpd_headwords", "sutta_title":"cakka 1"
            // SuttaBridge.get_word_html() needs the uid for dict_words table in dictionaries.sqlite3
            // where the form is "uid": "cakka-1/dpd" (spaces replaced with hyphens).
            uid = `${root.sutta_title.replace(/ /g, "-")}/dpd`;
        }

        // For empty UID, use loadHtml to avoid 404 from API endpoint
        if (uid === "") {
            var html = SuttaBridge.get_word_html(root.window_id, "");
            web.loadHtml(html);
            return;
        }

        const api_url = SuttaBridge.get_api_url();
        const enc_uid = uid.split("/").map(encodeURIComponent).join("/");
        web.url = `${api_url}/get_word_html_by_uid/${root.window_id}/${enc_uid}/`;
    }

    function load_book_spine_uid(spine_item_uid) {
        // Check if this is a PDF book
        const api_url = SuttaBridge.get_api_url();
        if (SuttaBridge.is_spine_item_pdf(spine_item_uid)) {
            // Load PDF viewer with file parameter
            const book_uid = SuttaBridge.get_book_uid_for_spine_item(spine_item_uid);
            const enc_book_uid = book_uid.split("/").map(encodeURIComponent).join("/");
            const pdf_url = `${api_url}/book_resources/${enc_book_uid}/document.pdf`;
            web.url = `${api_url}/assets/pdf-viewer/web/viewer.html?file=${encodeURIComponent(pdf_url)}`;
        } else {
            // Regular book content
            // Append anchor to URL for native browser scrolling (works on all platforms)
            // On same-page reloads, clear and re-set URL to trigger scroll
            const enc_spine_uid = spine_item_uid.split("/").map(encodeURIComponent).join("/");
            let url = `${api_url}/get_book_spine_item_html_by_uid/${root.window_id}/${enc_spine_uid}/`;
            if (root.anchor && root.anchor.length > 0) {
                // Ensure anchor has # prefix
                let anchor_fragment = root.anchor.startsWith('#') ? root.anchor : `#${root.anchor}`;
                url = `${url}${anchor_fragment}`;
            }
            web.url = url;
        }
    }

    // Whether this view is showing a sutta, as opposed to a dictionary word or
    // a book chapter. Mirrors the dispatch in Component.onCompleted.
    function is_sutta_content(): bool {
        return root.table_name !== "dict_words"
            && root.table_name !== "dpd_headwords"
            && root.table_name !== "dpd_roots"
            && root.table_name !== "bold_definitions"
            && root.table_name !== "book_spine_items";
    }

    function scroll_to_anchor() {
        if (root.anchor && root.anchor.length > 0) {
            // Remove the leading # if present
            let anchor_id = root.anchor.startsWith('#') ? root.anchor.substring(1) : root.anchor;

            // Sutta pages get window.ssp_jump_to_segment() from
            // simsapa.min.js: it walks the nearest preceding siblings when the
            // cited segment is absent, highlights where it landed, and inserts
            // the in-page notice.
            //
            // Word and book-chapter pages load the same bundle but must NOT use
            // it. Their anchors are plain element ids or names, not Bilara
            // segment ids, so the walk is meaningless there: a miss would scroll
            // the page to the top — undoing the native URL-fragment scroll this
            // JS pass exists only to reinforce — and plant a notice about a
            // location the reader never asked for. They take the direct lookup
            // below, which is the live path for every EPUB TOC entry whose
            // target carries a '#' fragment.
            let use_walk = root.is_sutta_content();
            let js = `
                (function() {
                    var requested = ${JSON.stringify(anchor_id)};
                    if (${use_walk} && typeof window.ssp_jump_to_segment === 'function') {
                        return window.ssp_jump_to_segment(requested);
                    }
                    var element = document.getElementById(requested);
                    if (element) {
                        element.scrollIntoView({ behavior: 'auto', block: 'start' });
                        return 'exact';
                    }
                    element = document.querySelector('a[name="' + requested + '"]');
                    if (element) {
                        element.scrollIntoView({ behavior: 'auto', block: 'start' });
                        return 'exact';
                    }
                    // The hash-form selector: this is the '#name' case, so it
                    // takes root.anchor with its leading '#' rather than the
                    // stripped id. A segment id contains a colon,
                    // which is valid in an id but not in a CSS selector fragment,
                    // so querySelector throws on it — that exception used to
                    // abort this whole function.
                    try {
                        element = document.querySelector(${JSON.stringify(root.anchor)});
                        if (element) {
                            element.scrollIntoView({ behavior: 'auto', block: 'start' });
                            return 'exact';
                        }
                    } catch (e) {
                    }
                    return 'missed';
                })();
            `;
            web.runJavaScript(js, function(result) {
                root.log_anchor_jump_result(anchor_id, result, use_walk);
            });
        }
    }

    // Distinguish "exact", "approximate" and "missed" in a support log, and
    // which of the two resolvers answered — the segment walk (sutta pages) or
    // the direct anchor lookup (word and book-chapter pages).
    function log_anchor_jump_result(anchor_id: string, result: var, used_walk: bool) {
        if (result === undefined || result === null) {
            return;
        }
        let outcome = String(result);
        let path = used_walk ? "walk" : "direct";
        logger.debug("scroll_to_anchor(): " + path + " -> " + outcome + " for " + anchor_id + " in " + root.item_uid);
        if (outcome.startsWith("fallback:")) {
            logger.info("scroll_to_anchor(): requested " + anchor_id + " not found in " + root.item_uid + ", used " + outcome.substring(9));
        } else if (outcome === "missed") {
            logger.warn("scroll_to_anchor(): no element for anchor " + anchor_id + " in " + root.item_uid);
        }
    }

    Component.onCompleted: {
        root.set_properties_from_data_json();
        // Dict words, DPD headwords/roots, and bold definitions all load via the word renderer
        if (root.table_name === "dict_words" || root.table_name === "dpd_headwords" || root.table_name === "dpd_roots" || root.table_name === "bold_definitions") {
            root.load_word_uid(root.item_uid);
        } else if (root.table_name === "book_spine_items") {
            root.load_book_spine_uid(root.item_uid);
        } else {
            root.load_sutta_uid(root.item_uid);
        }
    }

    // Load the sutta or dictionary word when the Loader in SuttaHtmlView updates data_json
    onData_jsonChanged: function() {
        root.set_properties_from_data_json();
        // Dict words, DPD headwords/roots, and bold definitions all load via the word renderer
        if (root.table_name === "dict_words" || root.table_name === "dpd_headwords" || root.table_name === "dpd_roots" || root.table_name === "bold_definitions") {
            root.load_word_uid(root.item_uid);
        } else if (root.table_name === "book_spine_items") {
            root.load_book_spine_uid(root.item_uid);
        } else {
            root.load_sutta_uid(root.item_uid);
        }
    }

    onIs_darkChanged: function() {
        let js = "";
        if (root.is_dark) {
            js = `
document.body.classList.add('dark');
document.documentElement.classList.add('dark');
document.documentElement.style.colorScheme = 'dark';
`;
        } else {
            js = `
document.body.classList.remove('dark');
document.documentElement.classList.remove('dark');
document.documentElement.style.colorScheme = 'light';
`;
        }
        web.runJavaScript(js);
    }

    onIs_reading_modeChanged: function() {
        let js = "";
        if (root.is_reading_mode) {
            js = `
if (document.getElementById('readingModeButton')) {
    document.getElementById('readingModeButton').classList.add('active');
}
`;
        } else {
            js = `
if (document.getElementById('readingModeButton')) {
    document.getElementById('readingModeButton').classList.remove('active');
}
`;
        }
        web.runJavaScript(js);
    }

    Connections {
        target: SuttaBridge

        function onShowBottomFootnotesChanged() {
            const enabled = SuttaBridge.get_show_bottom_footnotes();
            const js = `
if (document.SSP) {
    document.SSP.show_bottom_footnotes = ${enabled};
    if (window.footnote_bottom_bar_refresh) {
        window.footnote_bottom_bar_refresh();
    }
}`;
            web.runJavaScript(js);
        }
    }

    WebEngineRepaintNudge { web: web }

    WebEngineView {
        id: web
        anchors.fill: parent
        visible: root.visible
        enabled: root.visible

        onLoadingChanged: function(loadRequest) {
            if (loadRequest.status === WebEngineView.LoadStartedStatus) {
                root.load_in_progress = true;
            }
            if (root.is_dark) {
                web.runJavaScript("document.documentElement.style.colorScheme = 'dark';");
            }
            if (root.is_reading_mode) {
                let js = `
if (document.getElementById('readingModeButton')) {
    document.getElementById('readingModeButton').classList.add('active');
}
`;
                web.runJavaScript(js);
            }
            // Set footnote bottom bar setting from database
            const show_footnotes = SuttaBridge.get_show_bottom_footnotes();
            web.runJavaScript(`
if (document.SSP) {
    document.SSP.show_bottom_footnotes = ${show_footnotes};
}
`);
            if (loadRequest.status === WebEngineView.LoadSucceededStatus) {
                // Skip the spurious initial blank placeholder load that fires
                // from `web.loadHtml()` before the real URL load. It would
                // otherwise emit page_loaded on a transient DOM that is about
                // to be replaced, briefly activating the find bar before the
                // real page load clears it. QtWebEngine reports this load
                // with a non-http URL (`data:`, `about:blank`, or empty).
                let load_url = loadRequest.url.toString();
                if (!load_url.startsWith("http://") && !load_url.startsWith("https://")) {
                    return;
                }
                if (!root.load_in_progress) {
                    return;
                }
                root.load_in_progress = false;
                root.page_loaded();
                // Note: Anchor scrolling is now handled natively by including it in the URL
                //
                // The JavaScript fallback provides additional reliability in case:
                // - The anchor element has a different attribute (like `name` instead of `id`)
                // - There are timing issues with native scrolling
                // - The WebView needs a "nudge" to complete scrolling
                if (root.anchor && root.anchor.length > 0) {
                    scroll_timer.restart();
                }
            }
        }
    }
}
