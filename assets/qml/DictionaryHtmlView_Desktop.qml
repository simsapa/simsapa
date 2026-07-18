import QtQuick
import QtWebEngine

import com.profoundlabs.simsapa

Item {
    id: root
    anchors.fill: parent

    property string window_id
    property string word_uid
    property bool is_dark

    function load_word_uid(uid) {
        if (uid == "Word") {
            // Initial blank page
            uid = "";
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

    Component.onCompleted: {
        root.load_word_uid(root.word_uid);
    }

    onWord_uidChanged: function() {
        root.load_word_uid(root.word_uid);
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

    // TODO: implement find_bar
    // onFindTextFinished: function(result) {
    //     if (!find_bar.visible)
    //         find_bar.visible = true;
    //
    //     find_bar.numberOfMatches = result.numberOfMatches;
    //     find_bar.activeMatch = result.activeMatch;
    // }
    //
    // onLoadingChanged: function(loadRequest) {
    //     if (loadRequest.status == WebEngineView.LoadStartedStatus)
    //         find_bar.reset();
    // }

    WebEngineRepaintNudge { web: web }

    WebEngineView {
        id: web
        anchors.fill: parent
        visible: root.visible
        enabled: root.visible

        onLoadingChanged: {
            if (root.is_dark) {
                web.runJavaScript("document.documentElement.style.colorScheme = 'dark';");
            }
        }
    }
}
