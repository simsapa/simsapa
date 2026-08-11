import QtQuick
import QtWebView

import com.profoundlabs.simsapa

/*
 * Mobile WebView Visibility Management
 *
 * This component wraps QtWebView in an Item container to provide proper visibility control.
 *
 * On mobile platforms (Android/iOS), QtWebView uses native platform views (Android WebView,
 * WKWebView) that render in a separate layer above Qt Quick content. These native views don't
 * respect QML's visibility hierarchy, so simply setting visible: false on parent items doesn't
 * reliably hide them.
 *
 * Solution:
 * 1. Wrap WebView in an Item container that participates in QML's visibility hierarchy
 * 2. Explicitly bind WebView's visible property to the container's visibility
 * 3. Set enabled: false in addition to visible: false to stop native rendering
 *
 * This ensures the WebView is properly hidden when it should not be visible, preventing
 * blank yellow webviews from covering the screen.
 *
 * See docs/mobile-webview-visibility-management.md for detailed explanation.
 */

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
    // last emit. On Android QtWebView, a LoadSucceededStatus fires for the
    // target URL before the navigation actually starts (the old page's DOM
    // is still live) — without this gate, page_loaded would fire on the
    // stale DOM and any consumers (setSearchTerm, bookmark scroll, ...)
    // would act on the old page before it is replaced.
    property bool load_in_progress: false

    Timer {
        id: scroll_timer
        interval: 300
        repeat: false
        onTriggered: root.scroll_to_anchor()
    }

    /* Force the native WebView to re-send its size to Chromium.
     *
     * When the WordSummary pane closes, the SplitView gives this view its
     * height back. If Chromium does not act on that resize, the page keeps the
     * initial containing block it had while the pane was open, and everything
     * anchored to the viewport bottom (the column bar, the footnote bar) stays
     * pinned mid-screen. Ordinary block content is unaffected — its height is
     * content-driven — which is why sutta text still paints above *and* below
     * the stranded bar, and why neither scrolling nor re-opening the pane
     * recovers it: a compositor scroll re-runs no layout, and the next resize
     * is ignored the same way the first one was.
     *
     * Nothing inside the page can repair that, because every quantity JS reads
     * (window.innerHeight included) is stale too. A 1px geometry jiggle is a
     * resize Chromium has to observe. Same remedy as WebEngineRepaintNudge.qml
     * uses for the desktop stale-frame bug, applied to the native WebView.
     *
     * THE DELAY BEFORE THE JIGGLE IS LOAD-BEARING. It used to fire 50 ms after
     * the close, i.e. before the *ordinary* resize had been delivered — so on a
     * healthy device (Galaxy S23, Android 16) all 19 measured closes credited
     * the jiggle for what the ordinary resize had already done, and the two were
     * indistinguishable. The `pre_jiggle` report below is sent once the ordinary
     * resize has had time to land and before the jiggle fires; it is what lets
     * the log say nothing was wrong in the first place. Do not shorten it.
     */
    function nudge_webview_geometry() {
        // Logged from Qt's side of the boundary, because the page cannot see
        // any of it. Note this height is the *pre-layout* one — the SplitView
        // re-lays out in the polish pass — which is why the comparable geometry
        // is reported from the timers below instead.
        logger.info("VIEWPORT-NUDGE-QT: phase=close item=" + Math.round(root.width) + "x" + Math.round(root.height)
                    + " web=" + Math.round(web.width) + "x" + Math.round(web.height)
                    + " dpr=" + root.Screen.devicePixelRatio);
        pre_jiggle_report_timer.restart();
    }

    // Qt's height for the native view at call time. Logged as `qt_h0` only:
    // the SplitView re-lays out in the polish pass, so this is still the
    // summary-open height and must not be compared against anything. The
    // authoritative values are reported from the timers below.
    function webview_height(): real {
        return web.height;
    }

    // The page measures a phase on each of these reports, so QML owns the
    // timing of both. Two independently maintained sets of constants would
    // drift, and the whole attribution depends on the order being exact.
    function report_qt_geometry(phase: string) {
        web.runJavaScript(`if (typeof window.ssp_report_qt_geometry === 'function') { window.ssp_report_qt_geometry(${web.height}, ${root.Screen.devicePixelRatio}, '${phase}'); }`);
    }

    // Long enough for the ordinary resize to have been delivered. The page
    // measures its `natural` phase here — the phase that says whether anything
    // needed fixing at all.
    Timer {
        id: pre_jiggle_report_timer
        interval: 250
        repeat: false
        onTriggered: {
            logger.info("VIEWPORT-NUDGE-QT: phase=pre_jiggle web=" + Math.round(web.width) + "x" + Math.round(web.height)
                        + " dpr=" + root.Screen.devicePixelRatio);
            root.report_qt_geometry("pre_jiggle");
            geometry_nudge_timer.restart();
        }
    }

    Timer {
        id: geometry_nudge_timer
        interval: 50
        repeat: false
        onTriggered: {
            web.anchors.bottomMargin = 1;
            geometry_restore_timer.restart();
        }
    }

    Timer {
        id: geometry_restore_timer
        interval: 50
        repeat: false
        onTriggered: {
            web.anchors.bottomMargin = 0;
            post_jiggle_report_timer.restart();
        }
    }

    // Delayed past the restore so the resize it causes has been delivered
    // before the page measures the `jiggle` phase and runs its own repair.
    Timer {
        id: post_jiggle_report_timer
        interval: 50
        repeat: false
        onTriggered: {
            logger.info("VIEWPORT-NUDGE-QT: phase=post_jiggle web=" + Math.round(web.width) + "x" + Math.round(web.height)
                        + " dpr=" + root.Screen.devicePixelRatio);
            root.report_qt_geometry("post_jiggle");
        }
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

    function scroll_to_anchor() {
        if (root.anchor && root.anchor.length > 0) {
            // Remove the leading # if present
            let anchor_id = root.anchor.startsWith('#') ? root.anchor.substring(1) : root.anchor;

            // Try to scroll to the element with the anchor ID
            let js = `
                (function() {
                    var element = document.getElementById('${anchor_id}');
                    if (element) {
                        element.scrollIntoView({ behavior: 'auto', block: 'start' });
                        return true;
                    }
                    // Also try with querySelector in case it's a more complex selector
                    element = document.querySelector('a[name="${anchor_id}"]');
                    if (element) {
                        element.scrollIntoView({ behavior: 'auto', block: 'start' });
                        return true;
                    }
                    // Try with the hash directly
                    element = document.querySelector('${root.anchor}');
                    if (element) {
                        element.scrollIntoView({ behavior: 'auto', block: 'start' });
                        return true;
                    }
                    return false;
                })();
            `;
            web.runJavaScript(js);
        }
    }

    Component.onCompleted: {
        root.set_properties_from_data_json();
        // Dict words, DPD headwords, and bold definitions all load via the word renderer
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
        // Dict words, DPD headwords, and bold definitions all load via the word renderer
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

    WebView {
        id: web
        anchors.fill: parent
        visible: root.visible
        enabled: root.visible

        onLoadingChanged: function(loadRequest) {
            if (loadRequest.status === WebView.LoadStartedStatus) {
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
            if (loadRequest.status === WebView.LoadSucceededStatus) {
                // Skip non-http loads (transient `web.loadHtml()` placeholder
                // reported as data:/about:blank/empty).
                let load_url = loadRequest.url.toString();
                if (!load_url.startsWith("http://") && !load_url.startsWith("https://")) {
                    return;
                }
                // Android QtWebView fires a spurious LoadSucceededStatus for
                // the target URL BEFORE the real navigation starts (the old
                // page's DOM is still live). Require a LoadStartedStatus
                // since the last emit to filter that out.
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
