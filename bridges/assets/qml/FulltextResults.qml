pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

/* import com.profoundlabs.simsapa */
/* import data // for qml preview */

ColumnLayout {
    id: root

    required property bool is_dark
    // Mobile rendering troubleshooting toggles (see AppSettingsWindow.qml →
    // "Rendering"). Passed down from the parent window which reads them from
    // SuttaBridge. Default off so QML preview / desktop are unaffected.
    property bool render_use_flat_results_background: false
    property bool render_disable_results_clip: false
    property bool item_height_use_default: true
    property int item_height_fixed: 0
    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile
    readonly property string match_bg: root.is_dark ? "#007A31" : "#F6E600"

    Logger { id: logger }

    // Grouped, break-down-aware deconstruction state for the Dictionary DPD
    // Lookup (incl. Combined-remap) path. Populated by
    // set_search_result_page() from the SearchResultPage payload; empty for
    // every other search path (the selector row stays hidden).
    //
    // The break-down lock filter is authoritative in Rust: the selection index
    // and lock state ride along in the page request (SearchParams), and the
    // backend returns an already-ordered, already-filtered, densely paginated
    // page. This view only renders what it receives and re-requests page 0
    // whenever the selection or lock changes. Consequently bold-definition and
    // Fulltext-Match rows are no longer hidden by the lock — those streams
    // query the complete compound as typed and never deconstruct, so no
    // break-down choice applies to them. See
    // docs/search-snippet-highlight-pipeline.md.
    //
    // Selection + lock reset on new query only — the embedding window calls
    // reset_deconstructor_state() on query-text change, NOT on page navigation
    // (which must preserve the selection/lock).
    property var deconstructions: []
    property var direct_uids: []
    readonly property var deconstructor_words: {
        let out = [];
        for (let i = 0; i < root.deconstructions.length; i++) {
            out.push(root.deconstructions[i].words_joined);
        }
        return out;
    }
    property int selected_deconstruction_index: 0
    property bool deconstructor_locked: false

    // Reset the break-down selection + lock. Called by the embedding window on
    // a new query only (query-text change), never on page navigation.
    function reset_deconstructor_state() {
        root.selected_deconstruction_index = 0;
        root.deconstructor_locked = false;
    }

    /* BojjhangaData { id: results_model } // for qml preview */
    ListModel { id: results_model }

    function next_selectable_index(from: int, direction: int): int {
        var i = from + direction;
        while (i >= 0 && i < results_model.count) {
            var row = results_model.get(i);
            if (row && !row.is_section_header)
                return i;
            i += direction;
        }
        return from;
    }

    function select_previous_result() {
        fulltext_list.currentIndex = root.next_selectable_index(fulltext_list.currentIndex, -1)
    }

    function select_next_result() {
        fulltext_list.currentIndex = root.next_selectable_index(fulltext_list.currentIndex, +1)
    }

    readonly property int font_point_size: root.is_mobile ? 14 : 11
    readonly property TextMetrics tm1: TextMetrics { text: "#"; font.pointSize: root.font_point_size }

    required property var new_results_page_fn

    property var current_results: []
    property int page_len: 10
    property int page_num: 0
    property int total_hits: 0
    property int total_pages: (total_hits > 0 ? Math.ceil(total_hits / page_len) : 1)
    // Active "Exclude snippets containing" terms (cleaned, comma-joined), set by
    // the parent. When a page is empty because every snippet was excluded
    // (total_hits > 0 but no rows), the empty state names the filter instead of
    // saying "No results found.". See docs/search-snippet-highlight-pipeline.md.
    property string snippet_exclude_terms: ""
    property bool is_loading: false
    // Whether the app database/searcher have finished loading. Passed down from
    // the parent window. While loading we show the app logo + "Loading..."
    // (on mobile this panel is the main view the user waits on) instead of the
    // generic "No results found." empty state. Defaults true so QML preview /
    // desktop tooling are unaffected.
    property bool db_ready: true
    // "Suttas", "Dictionary" or "Library" — which area the current results
    // belong to. Used to pick the per-area block of the fulltext status, so a
    // failure is only reported to the user when it is a failure of the area
    // they actually searched.
    property string search_area: ""
    // The search mode the current results came from, exactly as the search
    // parameters spell it ("Fulltext Match", "Combined", "Contains Match",
    // "Title Match", "DPD Lookup", "Headword Match").
    //
    // Only the Tantivy-backed modes may be told that the index is at fault. The
    // others go through FTS5/SQLite and work perfectly on a volume where every
    // Tantivy index failed to open — that is the reporting user's exact
    // configuration, and telling them a Contains Match found nothing "because
    // the search index could not be opened" is a fabricated diagnosis of a
    // search that never touched the index. Fulltext PRD FR-23 scopes the
    // message to FulltextMatch/Combined for this reason.
    property string search_mode: ""
    // Set by check_fulltext_index_problem() when a page comes back empty and
    // the index for this area could not be opened. Empty string means "no
    // index problem to report", which is the overwhelmingly common case.
    property string fulltext_problem_message: ""
    // Returns the parsed fulltext status object (see
    // SuttaBridge.get_fulltext_status), or null when unavailable. Supplied by
    // the parent so this component need not import the bridge.
    property var fulltext_status_fn: null
    property alias currentIndex: fulltext_list.currentIndex
    property alias currentItem: fulltext_list.currentItem

    // Distinguish "no results" from "the index could not be opened".
    //
    // Only ever called when a page came back with no rows, so it costs nothing
    // on the normal path. It must **not** fire merely because an area has no
    // index: a user who never downloaded a language has no fault to report, and
    // telling them the index is broken would send them to Rebuild Search Index
    // for nothing.
    //
    // The gate is therefore the backend's `state`, not a zero count:
    // `could_not_open` means index directories exist, indexes were attempted,
    // and every one of them failed. See backend/src/fulltext_status.rs.
    //
    // It is the **searched area's** state, not the whole app's. The top-level
    // one reads `ready` as soon as anything opened anywhere, so a user whose
    // sutta indexes open and whose dictionary indexes all fail would be back to
    // a silent "No results found." on every dictionary search — the same defect,
    // narrowed to one area. The whole-app state is the fallback for a search
    // area this component cannot map (there is none today).
    //
    // The status arrives through a callback rather than a direct SuttaBridge
    // call, because this component deliberately does not import the bridge —
    // its `import com.profoundlabs.simsapa` is commented out so it stays usable
    // in QML preview, and everything else it needs is likewise passed down.
    // Same shape as new_results_page_fn.
    function check_fulltext_index_problem() {
        root.fulltext_problem_message = "";

        // The mode gate comes first: a mode that does not read the Tantivy
        // index has nothing to say about it, however broken it is. See
        // `search_mode` and `uses_fulltext_index()`.
        if (!root.uses_fulltext_index()) {
            return;
        }

        if (!root.fulltext_status_fn) {
            return;
        }

        const status = root.fulltext_status_fn(); // qmllint disable use-proper-function
        if (!status) {
            return;
        }

        // The area's own block when the search area maps to one, the whole-app
        // verdict otherwise. Both carry a ready-made sentence: every
        // user-facing string this feature can emit is written in
        // backend/src/fulltext_status.rs, so that one file can be checked for
        // jargon — and so is every decision about *when* to speak.
        //
        // An empty per-area message means "say nothing", which is the normal
        // case. Do not add conditions here: an area that opened some indexes and
        // failed on others has working search and incomplete results, and that
        // sentence is one the backend already knows how to write.
        const area = root.area_status(status);
        const message = area
            ? area.message
            : (status.state === "could_not_open" ? status.message : "");
        if (message.length === 0) {
            return;
        }

        root.fulltext_problem_message = message
            + " Open Database Validation from the menu for details.";
    }

    // Does the mode that produced these results read the Tantivy index at all?
    //
    // An **allowlist**, not a denylist of the FTS5 modes: a mode added later
    // stays silent by default, which is the failure that costs nothing. The
    // reverse — a new FTS5 mode silently inheriting "the search index could not
    // be opened" — is the defect this gate exists to prevent.
    //
    // "Combined" is included because the Dictionary combined page's third
    // stream is a Fulltext Match (see
    // docs/search-snippet-highlight-pipeline.md §9). An empty mode (QML
    // preview, or a page produced before any search) is treated as not using
    // the index, so the message never appears without a search behind it.
    function uses_fulltext_index(): bool {
        return root.search_mode === "Fulltext Match" || root.search_mode === "Combined";
    }

    // The status block for the area that was just searched, or null when the
    // area does not map to one.
    function area_status(status) {
        switch (root.search_area) {
        case "Suttas":     return status.sutta;
        case "Dictionary": return status.dict;
        case "Library":    return status.library;
        default:           return null;
        }
    }

    function set_search_result_page(search_result_page) {
        // SearchResultPage { total_hits, page_len, page_num, results,
        //   deconstructions?, direct_uids? } — the last two present only on the
        // Dictionary DPD Lookup path. Do NOT reset the selection/lock here; page
        // navigation re-delivers the same deconstructions and must preserve the
        // user's break-down choice. The embedding window resets on new query.
        let d = search_result_page;
        root.total_hits = d.total_hits;
        root.page_len = d.page_len;
        root.page_num = d.page_num;
        root.current_results = d.results;
        root.deconstructions = d.deconstructions || [];
        root.direct_uids = d.direct_uids || [];
        root.update_page();
    }

    function current_result_data(): var {
        return results_model.get(fulltext_list.currentIndex);
    }

    RowLayout {
        id: controls_row
        Layout.fillWidth: true

        SpinBox {
            id: fulltext_page_input; from: 1; to: 999;
            visible: false
            editable: true
            Layout.preferredWidth: 50
        }

        Button {
            id: fulltext_prev_btn
            Layout.preferredWidth: 40
            icon.source: "icons/32x32/fa_angle-left-solid.png"
            ToolTip.visible: hovered
            ToolTip.text: "Previous page of results"
            enabled: root.page_num > 0
            onClicked: {
                fulltext_list.positionViewAtBeginning();
                root.page_num--;
                root.new_results_page_fn(root.page_num); // qmllint disable use-proper-function
            }
        }
        Button {
            id: fulltext_next_btn
            Layout.preferredWidth: 40
            icon.source: "icons/32x32/fa_angle-right-solid.png"
            ToolTip.visible: hovered
            ToolTip.text: "Next page of results"
            enabled: root.page_num < root.total_pages - 1
            onClicked: {
                fulltext_list.positionViewAtBeginning();
                root.page_num++;
                root.new_results_page_fn(root.page_num); // qmllint disable use-proper-function
            }
        }

        Label {
            id: fulltext_label
            // TODO: Use result count range: Showing a-b out of x
            text: "Page " + (root.page_num+1) + " of " + root.total_pages
        }

        // Spacer
        Item {
            Layout.fillWidth: true
        }

        Button {
            id: fulltext_first_page_btn
            Layout.preferredWidth: 40
            icon.source: "icons/32x32/fa_angles-left-solid.png"
            ToolTip.visible: hovered
            ToolTip.text: "First page of results"
            enabled: root.page_num > 0
            onClicked: {
                fulltext_list.positionViewAtBeginning();
                root.page_num = 0;
                root.new_results_page_fn(root.page_num); // qmllint disable use-proper-function
            }
        }
        Button {
            id: fulltext_last_page_btn
            visible: false
            Layout.preferredWidth: 40
            icon.source: "icons/32x32/fa_angles-right-solid.png"
            ToolTip.visible: hovered
            ToolTip.text: "Last page of results"
            Layout.alignment: Qt.AlignRight
        }
    }

    // Break-down selector for Dictionary DPD Lookup results. Shown only when
    // the current query deconstructs. Locking filters the DPD result stream in
    // Rust before pagination, so a change of selection or lock changes both the
    // rows and the total — the view re-requests page 0 rather than re-filtering
    // in place. Selection/lock reset on new query only.
    DeconstructorSelector {
        id: deconstructor
        Layout.fillWidth: true
        // Match the paging buttons above (prev/next), sized to the default
        // Button height in controls_row.
        control_size: 40
        visible: root.deconstructor_words.length > 0
        model: root.deconstructor_words
        current_index: root.selected_deconstruction_index
        locked: root.deconstructor_locked
        // Picking a break-down auto-locks it, so the results filter to the
        // chosen break-down without a second click — matching GlossTab and
        // WordSummary. The lock stays independently toggleable afterwards. The
        // selector is emit-only, so these assignments are what drive its
        // visual state.
        onActivated: (index) => {
            root.selected_deconstruction_index = index;
            root.deconstructor_locked = true;
            root.page_num = 0;
            root.new_results_page_fn(root.page_num); // qmllint disable use-proper-function
        }
        onLock_toggled: (locked) => {
            root.deconstructor_locked = locked;
            root.page_num = 0;
            root.new_results_page_fn(root.page_num); // qmllint disable use-proper-function
        }
    }

    Rectangle {
        id: fulltext_loading_bar
        color: "transparent"
        Layout.fillWidth: true
        Layout.preferredHeight: 5
        Layout.alignment: Qt.AlignCenter
        AnimatedImage {
            source: "icons/gif/loading-bar.gif"
            anchors.horizontalCenter: parent.horizontalCenter
            visible: root.is_loading
            playing: root.is_loading
            cache: true
        }
    }

    // Pure derivation of the per-snippet find-bar query: the matched word (the
    // first `<span class='match'>`) plus the following 1–2 words, with HTML tags
    // stripped, ellipses and trailing punctuation dropped. Returns "" when the
    // snippet has no match span. On click this lets the find bar jump to *this*
    // snippet's passage instead of the original query. Kept pure for testing.
    // See docs/search-snippet-highlight-pipeline.md §7.
    function derive_find_query(snippet: string): string {
        if (!snippet) return "";
        var marker = "<span class='match'>";
        var idx = snippet.indexOf(marker);
        if (idx === -1) return "";
        // From the start of the matched word to the end of the snippet.
        var tail = snippet.substring(idx + marker.length);
        // Strip any remaining HTML tags (defensive; the refactor removed nesting).
        tail = tail.replace(/<[^>]*>/g, "");
        // Drop ellipses and collapse whitespace.
        tail = tail.replace(/…/g, " ").replace(/\s+/g, " ").trim();
        if (tail.length === 0) return "";
        // Matched word + up to 2 following words.
        var words = tail.split(" ").filter(function(w) { return w.length > 0; });
        var phrase = words.slice(0, 3).join(" ");
        // Drop trailing punctuation.
        phrase = phrase.replace(/[.,;:!?'"\)\]]+$/, "").trim();
        return phrase;
    }

    function update_page() {
        // Remove existing item selection.
        fulltext_list.currentIndex = -1;
        // Remove current list of items.
        results_model.clear()
        // Populate model with new items.
        root.total_pages = (root.total_hits > 0 ? Math.ceil(root.total_hits / root.page_len) : 1)
        // Header dedup: in "Show All Snippets" mode a record expands to several
        // adjacent rows sharing one uid; the metadata header is shown only on
        // the first row of each record group. A section-header row is a group
        // boundary, so the next real row always shows its header. See
        // docs/search-snippet-highlight-pipeline.md.
        // No break-down lock filter here: the backend already ordered and
        // filtered the DPD stream before paginating, so every row it sends is
        // meant to be rendered.
        var prev_uid = null;
        for (var i = 0; i < root.current_results.length; i++) {
            var item = root.current_results[i];
            var is_header = !!item.is_section_header;
            var show_header;
            if (is_header) {
                show_header = false;
                prev_uid = null;
            } else {
                show_header = (item.uid !== prev_uid);
                prev_uid = item.uid;
            }
            var result_data = {
                index: i,
                item_uid:    item.uid,
                table_name:  item.table_name,
                sutta_title: item.title,
                sutta_ref:   item.sutta_ref || "", // Can be 'None' from SearchResult::from_dict_word()
                snippet:     item.snippet,
                is_section_header: is_header,
                // Marks an expanded-snippet row vs. a whole-record row (see
                // docs/search-snippet-highlight-pipeline.md). Used by header
                // dedup / record grouping (Task 6.0).
                is_snippet:  !!item.is_snippet,
                // Show the metadata header only on the first row of a record
                // group (uid differs from the previous appended row).
                show_header: show_header,
                // Per-snippet find-bar query (matched word + following words),
                // read via current_result_data() on click. See
                // docs/search-snippet-highlight-pipeline.md §7.
                find_query:  root.derive_find_query(item.snippet),
                header_title: is_header ? item.title : "",
                /* author:      item.author, */
            };
            results_model.append(result_data);
        }
        // Reset scroll position — the model was cleared above, so any
        // previous scroll offset references items that no longer exist.
        fulltext_list.positionViewAtBeginning();

        // An empty page is the only case where the index's health is worth
        // asking about.
        if (results_model.count === 0) {
            root.check_fulltext_index_problem();
        } else {
            root.fulltext_problem_message = "";
        }
    }

    Text {
        id: empty_state
        // Three distinguishable empty states, most specific first:
        //   1. the index could not be opened — nothing was searched at all;
        //   2. records matched but every snippet on this page was removed by
        //      the exclusion filter — name the filter;
        //   3. the ordinary "No results found.".
        // Conflating (1) with (3) is what let a user run silently empty
        // searches for a whole session believing the texts simply had no match.
        text: root.fulltext_problem_message.length > 0
            ? root.fulltext_problem_message
            : ((root.total_hits > 0 && root.snippet_exclude_terms.trim().length > 0)
                ? "Results from this page were excluded by the filter: " + root.snippet_exclude_terms.trim()
                : "No results found.")
        // Don't show "No results found." while the DB is still loading — the
        // loading_state overlay shows the logo + "Loading..." instead.
        visible: root.db_ready && !root.is_loading && results_model.count === 0
        horizontalAlignment: Text.AlignHCenter
        font.italic: true
        color: "grey"
        wrapMode: Text.WordWrap
        Layout.fillWidth: true
    }

    // While the database is loading, show the app logo and a "Loading..."
    // message instead of the empty "No results found." state. On mobile this
    // panel is the main view the user is waiting on while the app loads.
    // This Item fills the space below the controls header; its content is
    // centered within that space and then lifted by half the header height
    // (controls_row + loading bar) so it sits centered over the whole panel.
    Item {
        id: loading_state
        visible: !root.db_ready
        Layout.fillWidth: true
        Layout.fillHeight: true

        ColumnLayout {
            spacing: 10
            anchors.centerIn: parent
            anchors.verticalCenterOffset: (-(controls_row.height + fulltext_loading_bar.height) / 2) - 10

            Image {
                source: "icons/appicons/simsapa.png"
                Layout.preferredWidth: 100
                Layout.preferredHeight: 100
                Layout.alignment: Qt.AlignHCenter
            }

            Text {
                text: "Loading..."
                font.pointSize: root.font_point_size
                // Use the system palette rather than root.is_dark: while loading,
                // the app theme hasn't been applied yet (is_dark defaults false),
                // but the window paints with the system palette — so honor that
                // to stay legible against a dark system UI.
                color: palette.text
                horizontalAlignment: Text.AlignHCenter
                Layout.alignment: Qt.AlignHCenter
            }
        }
    }

    ListView {
        id: fulltext_list
        orientation: ListView.Vertical

        readonly property int item_padding: 10
        readonly property int item_height: root.item_height_use_default
            ? root.tm1.height*4 + item_padding*2
            : root.item_height_fixed

        Layout.fillHeight: true
        Layout.fillWidth: true

        model: results_model
        clip: !root.render_disable_results_clip
        spacing: 0
        visible: results_model.count > 0
        delegate: search_result_delegate

        ScrollBar.vertical: ScrollBar {
            // AlwaysOn b/c mobile can't hover to show the bar
            policy: ScrollBar.AlwaysOn
            padding: 5
        }

        Keys.onPressed: function(event) {
            logger.info("key:" + event.key);
            if (event.key === Qt.Key_Up ||
                (event.key === Qt.Key_K && event.modifiers & Qt.ControlModifier)) {
                fulltext_list.currentIndex = root.next_selectable_index(fulltext_list.currentIndex, -1)
                event.accepted = true
            }
            else if (event.key === Qt.Key_Down ||
                        (event.key === Qt.Key_J && event.modifiers & Qt.ControlModifier)) {
                fulltext_list.currentIndex = root.next_selectable_index(fulltext_list.currentIndex, +1)
                event.accepted = true
            }
        }
    }

    Component {
        id: search_result_delegate
        ItemDelegate {
            id: result_item
            // NOTE: parent.width occasionally causes: TypeError: Cannot read property 'width' of null
            width: parent ? parent.width : 0
            height: result_item.is_section_header ? (root.tm1.height + 8) : fulltext_list.item_height

            required property int index
            required property string item_uid
            required property string table_name
            required property string sutta_title
            required property string sutta_ref
            required property string snippet
            required property bool is_section_header
            required property bool show_header
            required property string header_title
            /* required property string nikaya */
            property string author: ""
            /* required property int page_number */
            /* required property real score */

            // Section header rendering
            Text {
                anchors.fill: parent
                anchors.leftMargin: fulltext_list.item_padding
                anchors.rightMargin: fulltext_list.item_padding
                anchors.topMargin: 4
                anchors.bottomMargin: 4
                visible: result_item.is_section_header
                text: result_item.header_title
                font.bold: true
                font.pointSize: root.font_point_size + 2
                color: root.palette.active.text
                verticalAlignment: Text.AlignVCenter
            }

            // Real result rendering
            Frame {
                id: item_frame
                anchors.fill: parent
                padding: fulltext_list.item_padding
                visible: !result_item.is_section_header

                background: ListBackground {
                    is_dark: root.is_dark
                    use_flat_bg: root.render_use_flat_results_background
                    results_list: fulltext_list
                    result_item_index: result_item.index
                }

                MouseArea {
                    anchors.fill: parent
                    enabled: !result_item.is_section_header
                    onClicked: fulltext_list.currentIndex = result_item.index
                }

                ColumnLayout {
                    anchors.fill: parent
                    spacing: 4

                    // property color text_color: fulltext_list.currentIndex === result_item.index ? "#000" : "#fff"

                    // Title and metadata — shown only on the first row of a
                    // record group (header dedup; see
                    // docs/search-snippet-highlight-pipeline.md).
                    RowLayout {
                        spacing: 12
                        visible: result_item.show_header
                        Layout.fillWidth: true
                        Text {
                            text: result_item.sutta_ref
                            visible: result_item.sutta_ref !== ""
                            font.pointSize: root.font_point_size
                            font.bold: true
                            color: root.palette.active.text
                        }
                        // The title takes the remaining space and elides, so the
                        // uid on the right stays visible on narrow screens.
                        Text {
                            text: result_item.sutta_title
                            font.pointSize: root.font_point_size
                            font.bold: true
                            color: root.palette.active.text
                            // Elide towards the end of the title
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                        }
                        Text {
                            text: result_item.item_uid
                            font.pointSize: root.font_point_size
                            font.italic: true
                            color: root.palette.active.text
                            // Elide towards the beginning of the uid,
                            // which might be the same as other results items above and below,
                            // while the end of the uid is the translation author that distinguishes them
                            elide: Text.ElideLeft
                            Layout.maximumWidth: implicitWidth
                            Layout.minimumWidth: 0
                        }
                    }

                    // Snippet with highlighted HTML
                    Text {
                        id: item_snippet
                        color: root.palette.active.text
                        textFormat: Text.RichText
                        font.pointSize: root.font_point_size
                        text: "<style> span.match { background-color: %1; } </style>".arg(root.match_bg) + result_item.snippet
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                    }
                }
            }
        }
    }
}
