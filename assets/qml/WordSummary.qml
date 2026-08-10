pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

import com.profoundlabs.simsapa

Frame {
    id: root
    height: Math.min(root.window_height*0.5, min_height)

    required property bool is_dark
    property bool render_use_flat_results_background: false
    property bool render_disable_results_clip: false
    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile

    required property var handle_summary_close_fn
    required property var handle_open_dict_tab_fn

    readonly property int item_padding: 4
    property int min_height: summaries_model.count * (root.tm1.height*2 + item_padding*2) + 100
    required property int window_height

    readonly property int font_point_size: root.is_mobile ? 14 : 11
    readonly property TextMetrics tm1: TextMetrics { text: "#"; font.pointSize: root.font_point_size; font.bold: true }

    required property bool search_as_you_type_checked

    property alias search_btn: search_btn

    property bool is_loading: false
    property string current_query_id: ""

    // Grouped, break-down-aware lookup state (PRD FR-B3). `grouped_data` is the
    // parsed GroupedDpdLookup payload; `grouped_results` is the flat
    // {uid, word, summary} list before lock-filtering; `deconstructor_words` is
    // the list of break-down display strings for the selector.
    property var grouped_data: null
    property var grouped_results: []
    property var deconstructor_words: []
    property int selected_deconstruction_index: 0
    property bool deconstructor_locked: false

    Logger { id: logger }
    DeconstructorUtils { id: dec_utils }

    background: Rectangle {
        color: palette.window
        border.width: 1
        border.color: Qt.darker(palette.window, 1.15)
    }

    Timer {
        id: search_timer
        interval: 400 // milliseconds
        repeat: false
        onTriggered: {
            if (root.search_as_you_type_checked) {
                root.run_lookup(lookup_input.text);
            }
        }
    }

    ListModel { id: summaries_model }

    // Short-query offer for the dictionary lookup: append "/dpd" so a one/two
    // letter word is looked up as a dictionary uid (e.g. "ko" -> "ko/dpd").
    Dialog {
        id: short_query_dpd_dialog
        title: "Short Query"
        // Not Fusion's default header: in a Frame-rooted component like this
        // one, its unstated implicitHeight plus wrapping content makes the
        // dialog's implicitHeight oscillate on window resize. Same shape as the
        // two dialogs in SearchBarInput.qml. See DialogHeader.qml.
        header: DialogHeader { text: short_query_dpd_dialog.title }
        parent: Overlay.overlay
        anchors.centerIn: parent
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        width: Math.min(root.width - 40, 400)

        property string query: ""

        onAccepted: root.run_lookup(short_query_dpd_dialog.query + "/dpd", 1)

        contentItem: ColumnLayout {
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                text: "Short queries can return a very large number of results.\n\nLook up \"" + short_query_dpd_dialog.query + "\" as a dictionary word using the /dpd form (\"" + short_query_dpd_dialog.query + "/dpd\")?"
            }
        }
    }

    Connections {
        target: SuttaBridge

        function onDpdLookupGroupedReady(query_id: string, grouped_json: string) {
            // Ignore results from stale queries
            if (query_id !== root.current_query_id) {
                logger.info(`Discarding stale query results: ${query_id}, current: ${root.current_query_id}`);
                return;
            }

            root.is_loading = false;

            let grouped = JSON.parse(grouped_json);
            root.grouped_data = grouped;

            // Map SearchResult (uid/title/snippet) to the summary shape the list
            // consumes (uid/word/summary).
            let results = grouped.results || [];
            let mapped = [];
            for (let i=0; i < results.length; i++) {
                mapped.push({
                    uid: results[i].uid,
                    word: results[i].title,
                    summary: results[i].snippet,
                });
            }
            root.grouped_results = mapped;

            // Populate the break-down selector from the deconstructions.
            let decs = grouped.deconstructions || [];
            let words = [];
            for (let i=0; i < decs.length; i++) {
                words.push(decs[i].words_joined);
            }
            root.deconstructor_words = words;
            root.selected_deconstruction_index = 0;
            root.deconstructor_locked = false;

            root.refilter_summaries();
        }
    }

    // For qml preview
    /* ListModel { */
    /*     id: deconstructor_model */
    /*     ListElement { words_joined: "olokita + saññāṇena + eva" } */
    /*     ListElement { words_joined: "olokita + saññāṇena + iva" } */
    /* } */
    /* ListModel { */
    /*     id: summaries_model */
    /*     ListElement { summary: "<b>olokita</b> pp. <b>looked at, inspected</b> [ava + √lok], pp of oloketi" } */
    /*     ListElement { summary: "<b>saññāṇa 1</b> nt. <b>marking; signing</b> [saṁ + √ñā + aṇa], nt, act, from sañjānāti" } */
    /*     ListElement { summary: "<b>saññāṇa 2</b> nt. <b>mental noting;</b> lit. marking [saṁ + √ñā + aṇa], nt, act, from sañjānāti" } */
    /*     ListElement { summary: "<b>eva 1</b> ind. <b>only; just; merely; exclusively</b>, ind, emph" } */
    /*     ListElement { summary: "<b>iva 1</b> ind. <b>like; as</b>, ind" } */
    /* } */

    function set_query(query: string) {
        if (query.length < 4) {
            return;
        }
        lookup_input.text = query;
    }

    // Explicit "search" action from the search button (or Enter key). A 3-char
    // query runs immediately with no confirm (min_length 1 bypasses the
    // incremental floor, which run_lookup applies only for search-as-you-type).
    // A 1- or 2-char query is offered the /dpd uid form instead, so a one/two
    // letter word like "i" or "ko" is looked up as a dictionary word
    // ("ko" -> "ko/dpd"). An empty query does nothing.
    function request_lookup() {
        const q = lookup_input.text;
        if (q.length === 0) return;
        if (q.length >= 3) {
            root.run_lookup(q, 1);
            return;
        }
        short_query_dpd_dialog.query = q;
        short_query_dpd_dialog.open();
    }

    // Rebuild summaries_model from the grouped results, applying the lock
    // filter. Unlocked shows the full list (today's behavior); locked shows the
    // selected break-down's components plus direct matches. See PRD FR-B3.
    function refilter_summaries() {
        let visible = dec_utils.visible_uids(root.grouped_data,
                                             root.selected_deconstruction_index,
                                             root.deconstructor_locked);
        summaries_model.clear();
        let results = root.grouped_results;
        for (let i=0; i < results.length; i++) {
            if (dec_utils.uid_is_visible(visible, results[i].uid)) {
                summaries_model.append(results[i]);
            }
        }
        // clear the previous selection highlight
        summaries_list.currentIndex = -1;
    }

    // min_length 4 is the search-as-you-type floor (a plain text query runs from
    // 4 characters); the search button calls this with min_length 1.
    function run_lookup(query: string, min_length = 4) {
        if (query.length < min_length)
            return;

        // Generate a unique query ID using timestamp
        root.current_query_id = new Date().toISOString() + "_" + Math.random().toString(36).substring(2, 9);

        root.is_loading = true;

        // The grouped async lookup returns both the summaries and the
        // deconstructor break-downs in one payload (see onDpdLookupGroupedReady).
        SuttaBridge.dpd_lookup_grouped_json_async(root.current_query_id, query);
    }

    ColumnLayout {
        id: main_col
        anchors.fill: parent

        RowLayout {
            id: row_one
            TextField {
                id: lookup_input
                Layout.fillWidth: true
                text: ""

                font.pointSize: root.is_mobile ? 14 : 12

                // Suppress Sentence-case auto-capitalisation only (lookups are
                // lowercased in the backend anyway — `dpd_lookup*` normalize
                // their query text). Never Qt.ImhPreferLowercase.
                // See docs/android-soft-keyboard.md §4.
                inputMethodHints: Qt.ImhNoAutoUppercase
                EnterKey.type: Qt.EnterKeySearch

                onAccepted: search_btn.clicked()
                onTextChanged: {
                    if (root.search_as_you_type_checked) search_timer.restart();
                }
                selectByMouse: true

                // Reliably raise the Android/ChromeOS soft keyboard on the
                // first tap. See docs/android-soft-keyboard.md.
                MobileKeyboardHelper {}
            }
            Button {
                id: search_btn
                icon.source: root.is_loading ? "icons/32x32/fa_stopwatch-solid.png" : "icons/32x32/bx_search_alt_2.png"
                enabled: !root.is_loading
                onClicked: root.request_lookup()
                Layout.preferredHeight: lookup_input.height
                Layout.preferredWidth: lookup_input.height
                ToolTip.visible: hovered
                ToolTip.text: root.is_loading ? "Processing..." : "Search"
            }
            Button {
                id: close_btn
                icon.source: "icons/32x32/mdi--close.png"
                Layout.preferredHeight: lookup_input.height
                Layout.preferredWidth: lookup_input.height
                ToolTip.visible: hovered
                ToolTip.text: "Close word summaries"
                onClicked: root.handle_summary_close_fn() // qmllint disable use-proper-function
            }
        }

        RowLayout {
            id: row_two
            visible: root.deconstructor_words.length > 0
            // Picking a break-down auto-locks it, so the summaries filter to the
            // chosen break-down without a second click — matching GlossTab. The
            // lock stays independently toggleable afterwards. The selector is
            // emit-only, so these assignments are what drive its visual state.
            DeconstructorSelector {
                id: deconstructor
                Layout.fillWidth: true
                // Match the sibling buttons in this row (e.g. "Copy listed
                // summaries"), which are sized to lookup_input.height.
                control_size: lookup_input.height
                model: root.deconstructor_words
                current_index: root.selected_deconstruction_index
                locked: root.deconstructor_locked
                onActivated: (index) => {
                    root.selected_deconstruction_index = index;
                    root.deconstructor_locked = true;
                    root.refilter_summaries();
                }
                onLock_toggled: (locked) => {
                    root.deconstructor_locked = locked;
                    root.refilter_summaries();
                }
            }
            Button {
                id: copy_btn
                icon.source: "icons/32x32/lucide-lab--copy-text.png"
                Layout.preferredHeight: lookup_input.height
                Layout.preferredWidth: lookup_input.height
                ToolTip.visible: hovered
                ToolTip.text: "Copy listed summaries"
            }
            Button {
                id: open_lookup_window_btn
                icon.source: "icons/32x32/bxs_book_content.png"
                Layout.preferredHeight: lookup_input.height
                Layout.preferredWidth: lookup_input.height
                ToolTip.visible: hovered
                ToolTip.text: "Open query in Word Lookup Window"
            }
        }

        ListView {
            id: summaries_list
            orientation: ListView.Vertical
            clip: !root.render_disable_results_clip
            spacing: 0

            readonly property int item_height: root.tm1.height*2 + root.item_padding*2

            // FIXME: can't get this ListView to resize to fill the available height
            Layout.preferredHeight: root.height - row_one.height - row_two.height - item_height
            Layout.fillWidth: true

            model: summaries_model
            delegate: summaries_delegate

            ScrollBar.vertical: ScrollBar {
                policy: ScrollBar.AlwaysOn
                padding: 0
            }
        }

        Component {
            id: summaries_delegate
            ItemDelegate {
                id: result_item
                width: parent ? parent.width : 0
                height: summaries_list.item_height

                required property int index
                required property string uid
                required property string word
                required property string summary

                Frame {
                    id: item_frame
                    anchors.fill: parent
                    padding: root.item_padding

                    background: ListBackground {
                        is_dark: root.is_dark
                        use_flat_bg: root.render_use_flat_results_background
                        results_list: summaries_list
                        result_item_index: result_item.index
                    }

                    MouseArea {
                        anchors.fill: parent
                        onClicked: summaries_list.currentIndex = result_item.index
                    }

                    RowLayout {
                        anchors.fill: parent
                        spacing: 0
                        Text {
                            text: `<b>${result_item.word}</b> ${result_item.summary}`
                            color: root.palette.active.text
                            textFormat: Text.RichText
                            font.pointSize: root.font_point_size
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Button {
                            id: show_word_in_dict_tab
                            icon.source: "icons/32x32/bxs_book_content.png"
                            // NOTE: result_item.uid is the numerical dpd id, but dictionaries.sqlite3 has uid based on word lemma_1.
                            // Sanitize: dict_words.uid replaces spaces with hyphens (e.g. "gacchati 1" -> "gacchati-1/dpd").
                            onClicked: root.handle_open_dict_tab_fn(result_item.word.replace(/ /g, "-") + "/dpd") // qmllint disable use-proper-function
                            Layout.preferredHeight: lookup_input.height
                            Layout.preferredWidth: lookup_input.height
                            Layout.rightMargin: 5
                        }
                    }
                }
            }
        }
    }
}
