import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

import com.profoundlabs.simsapa

Frame {
    id: root
    Layout.fillWidth: true
    Layout.minimumHeight: root.icon_size

    Logger { id: logger }

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile

    required property int window_width
    required property bool is_wide
    required property bool is_tall
    required property bool db_loaded
    required property bool searcher_ready
    readonly property bool db_ready: root.db_loaded && root.searcher_ready
    required property var handle_query_fn
    required property Timer search_timer
    required property DrawerMenu mobile_menu
    required property bool search_as_you_type_checked
    required property bool is_loading
    required property bool has_query_error

    required property int icon_size

    property alias search_input: search_input
    property alias search_mode_dropdown: search_mode_dropdown
    property alias language_filter_dropdown: language_filter_dropdown
    property alias advanced_options_btn: advanced_options_btn

    // Emitted when the info button is clicked; the parent window opens the
    // Search Help window.
    signal helpRequested()

    // Search area state: "Suttas", "Dictionary", or "Library"
    property string search_area: "Suttas"
    readonly property var search_area_list: ["Suttas", "Dictionary", "Library"]

    function set_search_area(area: string) {
        if (search_area_list.indexOf(area) !== -1) {
            search_area = area;
        }
    }

    function cycle_search_area() {
        const current_index = search_area_list.indexOf(search_area);
        const next_index = (current_index + 1) % search_area_list.length;
        search_area = search_area_list[next_index];
    }

    background: Rectangle {
        color: "transparent"
        border.color: "transparent"
        border.width: 0
    }

    // Build the language dropdown model for the given area from the distinct
    // language values in the database. The same on-demand distinct-value query
    // is used for every area (Suttas, Dictionary, Library) for consistency.
    // Index 0 is the "Language"/"Lang" sentinel meaning "no language filter".
    // Index restoration is owned by the dropdown's restore_for_current_area().
    function load_language_labels_for_area(area: string) {
        let lang_labels;
        if (area === "Suttas") {
            lang_labels = SuttaBridge.get_sutta_language_labels();
        } else if (area === "Library") {
            lang_labels = SuttaBridge.get_library_language_labels();
        } else {
            // Dictionary: filter by the languages present in the dictionaries DB.
            lang_labels = SuttaBridge.get_dict_language_labels();
        }
        // Shorter first label for narrow screens.
        const first_label = root.is_wide ? "Language" : "Lang";
        language_filter_dropdown.model = [first_label].concat(lang_labels);
    }

    // EXACTLY ONE query per area switch.
    //
    // Both dropdowns restore their per-area mode/language independently via
    // their own `Connections { onSearch_areaChanged }` (pure restores — no
    // query). The single query is fired by `area_query_coordinator`. A second
    // query would cost real compute and can cause slowdown.
    //
    // Do not rely on the order of these handlers, nor on the order of the
    // Component.onCompleted handlers on initial load (Qt leaves both
    // unspecified): measured, the coordinator's query runs BEFORE the
    // dropdowns' own restore handlers, and root's onCompleted before theirs.
    // So every one of these entry points calls ensure_dropdowns_restored()
    // first: whichever runs first restores, the rest find it done. The query
    // then reads search_mode_dropdown.mode_for_query() and
    // language_filter_dropdown.language_for_query().
    Component.onCompleted: {
        logger.info("STARTUP-TRACE: SearchBarInput onCompleted start");
        // Keyboard diagnostics: log the detected platform once at startup so we
        // can confirm whether a Chromebook (Android app) is treated as mobile.
        logger.debug("SearchBarInput: Qt.platform.os=" + Qt.platform.os
            + " is_mobile=" + root.is_mobile + " is_desktop=" + root.is_desktop
            + " search_input.focus=" + search_input.focus
            + " inputMethod.visible=" + Qt.inputMethod.visible); // qmllint disable missing-property
        root.ensure_dropdowns_restored();
        root.handle_query_fn(search_input.text); // qmllint disable use-proper-function
    }

    // Restore each dropdown's saved selection for the current area, unless it
    // is already applied. Idempotent, so it is safe to call from every handler
    // that reacts to an area change or to completion. Skipping an applied
    // dropdown matters: a restore re-reads the process-global saved value,
    // which another window may have changed, and the language restore runs a
    // distinct-values query for its labels.
    function ensure_dropdowns_restored() {
        if (search_mode_dropdown.applied_area !== root.search_area) {
            search_mode_dropdown.restore_for_current_area();
        }
        if (language_filter_dropdown.applied_area !== root.search_area) {
            language_filter_dropdown.restore_for_current_area();
        }
    }

    function user_typed() {
        // TODO self._show_search_normal_icon()
        if (root.search_as_you_type_checked) root.search_timer.restart();
    }

    // Explicit "search" action from the search button (or Enter key). For a
    // query of 3+ chars, run it immediately (min_length 1 = "search anyway",
    // bypassing the incremental-search floor). For a short query (< 3 chars),
    // warn the user before running it, because short queries return many
    // results — in the Dictionary area we instead offer the /dpd uid form so a
    // one/two-letter word like "i" or "ko" resolves to its dictionary page.
    function request_search() {
        const q = search_input.text;
        if (q.length === 0) return;
        if (q.length >= 3) {
            root.handle_query_fn(q, 1); // qmllint disable use-proper-function
            return;
        }
        if (root.search_area === "Dictionary") {
            short_query_dpd_dialog.query = q;
            short_query_dpd_dialog.open();
        } else {
            short_query_warn_dialog.query = q;
            short_query_warn_dialog.open();
        }
    }

    // Short-query warning for the Suttas / Library areas: confirm and run the
    // query anyway (min_length 1 bypasses the incremental floor).
    Dialog {
        id: short_query_warn_dialog
        title: "Short Query"
        // Not Fusion's default header: with wrapping content it makes the
        // dialog's implicitHeight oscillate on every window resize. See
        // DialogHeader.qml.
        header: DialogHeader { text: short_query_warn_dialog.title }
        parent: Overlay.overlay
        anchors.centerIn: parent
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        width: Math.min(root.window_width - 40, 400)

        property string query: ""

        onAccepted: root.handle_query_fn(short_query_warn_dialog.query, 1) // qmllint disable use-proper-function

        // `width: parent.width` is what makes wrapMode work: a Dialog's
        // declared children are parented to popupItem->contentItem(), which is
        // already sized to availableWidth.
        Label {
            width: parent.width
            wrapMode: Text.WordWrap
            text: "Short queries can return a large number of results and may be slow.\n\nSearch anyway?"
        }
    }

    // Short-query offer for the Dictionary area: append "/dpd" so a one/two
    // letter word is looked up as a dictionary uid (e.g. "ko" -> "ko/dpd").
    Dialog {
        id: short_query_dpd_dialog
        title: "Short Query"
        // See the note on short_query_warn_dialog's header above.
        header: DialogHeader { text: short_query_dpd_dialog.title }
        parent: Overlay.overlay
        anchors.centerIn: parent
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        width: Math.min(root.window_width - 40, 400)

        property string query: ""

        onAccepted: root.handle_query_fn(short_query_dpd_dialog.query + "/dpd", 1) // qmllint disable use-proper-function

        Label {
            width: parent.width
            wrapMode: Text.WordWrap
            text: "Short queries can return a large number of results.\n\nLook up \"" + short_query_dpd_dialog.query + "\" as a dictionary word using the /dpd form (\"" + short_query_dpd_dialog.query + "/dpd\")?"
        }
    }

    Flow {
        id: search_bar_layout
        width: parent.width
        spacing: 5

        RowLayout {
            id: search_input_layout
            /* On wide screens, constrain the search input to 600px width so the
             * options can sit beside it. On narrow screens, let it take full
             * width, which will push the options to wrap below. */
            width: root.is_wide ? 600 : parent.width

            // Open/close the drawer menu on mobile
            Button {
                id: show_menu
                visible: root.is_mobile
                icon.source: "icons/32x32/mdi--menu.png"
                Layout.preferredHeight: root.icon_size
                Layout.preferredWidth: root.icon_size
                ToolTip.visible: hovered
                ToolTip.text: "Show Menu"
                onClicked: root.mobile_menu.open()
            }

            // === Search Input ====
            TextField {
                id: search_input
                enabled: root.db_ready
                Layout.fillWidth: true
                Layout.preferredWidth: root.is_wide ? 500 : 250
                Layout.preferredHeight: root.icon_size

                // Auto-focus on desktop so the user can type immediately. On
                // mobile do NOT pre-grab focus: with `focus: true` the field
                // already holds active focus once the DB finishes loading, so
                // the first physical tap is not a focus transition and Android
                // never raises the soft keyboard (needing a second tap).
                // Leaving it unfocused makes the first tap a real focus change.
                focus: root.is_desktop
                // Suppress the IME's Sentence-case auto-capitalisation, so a
                // romanised query looks lowercase — a cue to the user that
                // search is case-insensitive (it is: SearchQueryTask::new()
                // normalizes every mode and the DPD lookups lowercase their
                // input, so this is purely cosmetic).
                //
                // Do NOT add Qt.ImhPreferLowercase — inert on Android, but it
                // forces the lowercase layer under Qt Virtual Keyboard.
                //
                // This hint was removed for one build while chasing the
                // Gboard/Thai mid-word Shift bug and is now restored: removing
                // it did NOT fix Thai, so it is not implicated. Do not re-remove
                // it without new evidence. See docs/android-soft-keyboard.md §4.
                inputMethodHints: Qt.ImhNoAutoUppercase
                // Make the soft keyboard's action key a "Search" button. On
                // Android this maps to IME_ACTION_SEARCH, which is consistent
                // across taps (otherwise the first focus can show a "Next"
                // arrow that does not emit `accepted`) and fires `onAccepted`
                // when pressed, starting the query.
                EnterKey.type: Qt.EnterKeySearch
                font.pointSize: root.is_mobile ? 14 : 12
                placeholderText: {
                    if (!root.db_ready) return "Loading...";
                    if (root.search_area === "Dictionary") return "Search in dictionary";
                    if (root.search_area === "Library") return "Search in library";
                    return "Search in suttas";
                }

                onAccepted: search_btn.clicked()
                onTextChanged: root.user_typed()
                selectByMouse: true

                // Keyboard diagnostics: report focus transitions so we can see
                // whether tapping the field actually moves active focus to it
                // (the precondition for the IME to be raised).
                onActiveFocusChanged: logger.debug("search_input: activeFocus="
                    + search_input.activeFocus + " inputMethod.visible="
                    + Qt.inputMethod.visible) // qmllint disable missing-property

                // Reliably raise the Android/ChromeOS soft keyboard on the
                // first tap. See docs/android-soft-keyboard.md.
                MobileKeyboardHelper {}
            }

            Button {
                id: search_btn
                icon.source: root.has_query_error ? "icons/32x32/fa_triangle-exclamation-solid.png" : (root.is_loading ? "icons/32x32/fa_stopwatch-solid.png" : "icons/32x32/bx_search_alt_2.png")
                enabled: search_input.text.length > 0
                onClicked: root.request_search()
                Layout.preferredHeight: root.icon_size
                Layout.preferredWidth: root.icon_size
            }
        }

        RowLayout {
            id: search_options_layout

            Button {
                id: advanced_options_btn
                checkable: true
                icon.source: "icons/32x32/system-uicons--settings.png"
                Layout.preferredHeight: root.icon_size
                Layout.preferredWidth: root.icon_size
                ToolTip.visible: hovered
                ToolTip.text: "Advanced search options"
            }

            // Search area buttons (S = Suttas, D = Dictionary, L = Library)
            Row {
                id: search_area_buttons
                spacing: 0

                Button {
                    id: btn_suttas
                    text: "S"
                    checked: root.search_area === "Suttas"
                    checkable: true
                    autoExclusive: true
                    implicitWidth: root.icon_size
                    implicitHeight: root.icon_size
                    ToolTip.visible: hovered
                    ToolTip.text: "Suttas"
                    onClicked: root.search_area = "Suttas"
                }

                Button {
                    id: btn_dictionary
                    text: "D"
                    checked: root.search_area === "Dictionary"
                    checkable: true
                    autoExclusive: true
                    implicitWidth: root.icon_size
                    implicitHeight: root.icon_size
                    ToolTip.visible: hovered
                    ToolTip.text: "Dictionary"
                    onClicked: root.search_area = "Dictionary"
                }

                Button {
                    id: btn_library
                    text: "L"
                    checked: root.search_area === "Library"
                    checkable: true
                    autoExclusive: true
                    implicitWidth: root.icon_size
                    implicitHeight: root.icon_size
                    ToolTip.visible: hovered
                    ToolTip.text: "Library"
                    onClicked: root.search_area = "Library"
                }
            }

            ComboBox {
                id: search_mode_dropdown
                Layout.preferredHeight: root.icon_size
                Layout.preferredWidth: root.is_wide ? 120 : 80

                readonly property var search_mode_label_wide: {
                    "Suttas": [
                        "Fulltext Match",
                        "Contains Match",
                        "Title Match",
                    ],
                    "Library": [
                        "Fulltext Match",
                        "Contains Match",
                        "Title Match",
                    ],
                    "Dictionary": [
                        "Combined",
                        "DPD Lookup",
                        "Fulltext Match",
                        "Contains Match",
                        "Headword Match",
                    ],
                }

                // For a narrow screen, the CLOSED control shows shorter label
                // texts (see displayText below). The drop-down always lists the
                // wide ones — the popup is the surface with room, and Fusion
                // draws the two from different sources, so they can differ:
                // the delegate's text is `model[control.textRole]`
                // (Fusion/ComboBox.qml:30) while the closed control's
                // contentItem text is `control.displayText` (:50).
                //
                // Value reading uses get_text(), which returns the longer label
                // text, which is used for the JSON search parameters.
                readonly property var search_mode_label_narrow: {
                    "Suttas": [
                        "Fulltext",
                        "Contains",
                        "Title",
                    ],
                    "Library": [
                        "Fulltext",
                        "Contains",
                        "Title",
                    ],
                    "Dictionary": [
                        "Combined",
                        "Lookup",
                        "Fulltext",
                        "Contains",
                        "Headword",
                    ],
                }

                // When true, suppress side-effects (persistence + query) of
                // currentIndex changes caused by programmatic restores rather
                // than by the user.
                property bool suppress_persist: false

                // Tracks the area whose saved mode is currently applied, so a
                // currentIndex change that arrives mid-area-switch — the model
                // has rebound but restore_for_current_area() has not run yet —
                // is not mistaken for a user choice. (It also used to cover
                // is_wide-driven model swaps; this dropdown no longer has any,
                // since the model is always the wide list.)
                property string applied_area: ""

                // The model is ALWAYS the wide list, at every width. The narrow
                // labels are applied to the closed control only, via
                // displayText. Both lists have the same length per area, so
                // currentIndex means the same thing in either.
                //
                // This also makes textAt(i) return the wide label
                // unconditionally, which is what the run_*_dictionary_query()
                // callers in SuttaSearchWindow.qml match against.
                //
                // The model is assigned in restore_for_current_area(), not bound
                // to root.search_area: assigning a model resets currentIndex to
                // 0 (QQuickComboBox::setModel), and a binding re-evaluates in
                // whatever order the search_areaChanged handlers happen to run
                // — after the restore, it would wipe the restored index and the
                // reset would be saved as the user's choice.

                // Abbreviate the closed control on a narrow screen. Falls back
                // to currentText while currentIndex is out of range, which it
                // briefly is when the model is rebound on an area switch.
                displayText: {
                    if (root.is_wide) {
                        return currentText;
                    }
                    const narrow = search_mode_label_narrow[root.search_area];
                    if (currentIndex < 0 || currentIndex >= narrow.length) {
                        return currentText;
                    }
                    return narrow[currentIndex];
                }

                // The saved search mode for an area, or the area's first mode
                // when nothing valid is saved.
                function mode_for_area(area: string): string {
                    const wide_list = search_mode_label_wide[area];
                    const saved_mode = SuttaBridge.get_last_search_mode(area);
                    return wide_list.indexOf(saved_mode) !== -1 ? saved_mode : wide_list[0];
                }

                // The mode a query must run with.
                //
                // Until restore_for_current_area() has run for the current area,
                // currentIndex belongs to the previous area or to nothing, so
                // the saved mode is used — it is exactly what the restore is
                // about to show. The SearchBarInput entry points restore before
                // querying, so this branch is a fallback for other callers.
                //
                // Once restored, this window's own selection is used and NOT
                // the saved mode: the saved mode is process-global, shared by
                // every open search window, so another window choosing a mode
                // would otherwise change this window's queries while its
                // dropdown still shows its own choice.
                function mode_for_query(): string {
                    if (applied_area !== root.search_area) {
                        return mode_for_area(root.search_area);
                    }
                    return get_text();
                }

                // Pure restore (no query): sets the model for the area and the
                // saved mode's index. Call through root.ensure_dropdowns_restored().
                // The area-switch query is fired separately by root's
                // area_query_coordinator.
                function restore_for_current_area() {
                    const wide_list = search_mode_label_wide[root.search_area];
                    suppress_persist = true;
                    model = wide_list;
                    currentIndex = wide_list.indexOf(mode_for_area(root.search_area));
                    suppress_persist = false;
                    applied_area = root.search_area;
                }

                Component.onCompleted: {
                    recompute_widest_label_width();
                    root.ensure_dropdowns_restored();
                }

                Connections {
                    target: root
                    // Driven by area change (not by model change), because
                    // Suttas and Library share identical model labels — a
                    // Suttas↔Library switch produces no model-change signal,
                    // and a Dictionary→shorter-area switch may emit
                    // model-change after ComboBox auto-clips currentIndex.
                    function onSearch_areaChanged() {
                        root.ensure_dropdowns_restored();
                    }
                }

                onCurrentIndexChanged: {
                    if (suppress_persist) return;
                    // Mid-transition between search areas: the model just
                    // rebound and ComboBox auto-clipped currentIndex into the
                    // new (shorter) list before the area-restore could run.
                    // Ignore — restore_for_current_area(), called from the
                    // onSearch_areaChanged Connections above, sets the correct
                    // index for the new area. (Not from onModelChanged: that
                    // handler only re-measures the popup width, and a
                    // Suttas↔Library switch emits no model change at all.)
                    if (applied_area !== root.search_area) return;
                    SuttaBridge.set_last_search_mode(root.search_area, get_text());
                    root.handle_query_fn(search_input.text); // qmllint disable use-proper-function
                }

                function get_text(): string {
                    // Return the value using the wide values which is expected for JSON search parameters.
                    return search_mode_label_wide[root.search_area][currentIndex];
                }

                // --- popup width -------------------------------------------
                //
                // Fusion gives the popup `width: control.width`
                // (Fusion/ComboBox.qml:114), which is 80 px on a phone and
                // leaves 66 px for text once the popup's `padding: 1` and the
                // MenuItem delegate's `padding: 6` are taken off. The wide
                // labels need more than that ("Headword Match" measures ~97 px
                // at desktop metrics), so the popup is widened to fit them.
                //
                // This can only ever GROW the popup. It is not platform-gated,
                // and neither is displayText above, so be precise about what
                // that means on desktop: at is_wide the control is already
                // 120 px against ~111 px of widest label, so nothing changes —
                // but desktop is_wide is `width > 650` (SuttaSearchWindow.qml:58),
                // and a desktop window narrower than that takes the same branch a
                // phone does: abbreviated closed control, wide labels in a
                // widened drop-down. That is intended (the popup is the surface
                // with room, at any width), not an accident of leaving the gate
                // off.
                //
                // Only the WIDTH is clamped here, not the popup's x. Qt pushes a
                // popup inside the window only when its margins are >= 0 or it
                // has an implicitWidth (qquickpopuppositioner.cpp:174-213), and
                // Fusion's ComboBox popup sets only topMargin/bottomMargin and no
                // implicitWidth — so a popup wider than the room to its right
                // would be clipped at the window edge rather than shifted left.
                // Not reachable today: language_filter_dropdown (80 px) and
                // search_help_btn sit to the right of this control, which absorbs
                // the ~30-50 px of growth. Re-check it if a longer mode label is
                // ever added.
                property int widest_label_width: 0

                // Measured rather than guessed, because the Android default font
                // is larger than the desktop one and a hard-coded width would be
                // wrong on one of them.
                //
                // Deliberately a function and a plain property, NOT a binding:
                // measuring each label means assigning `text`, and reading
                // advanceWidth inside a binding that also writes text is a
                // binding loop.
                TextMetrics {
                    id: mode_label_metrics
                    font: search_mode_dropdown.font
                    onFontChanged: search_mode_dropdown.recompute_widest_label_width()
                }

                function recompute_widest_label_width() {
                    // onFontChanged can fire before search_area is set.
                    const labels = search_mode_label_wide[root.search_area];
                    if (!labels) {
                        return;
                    }
                    let w = 0;
                    for (let i = 0; i < labels.length; i++) {
                        mode_label_metrics.text = labels[i];
                        w = Math.max(w, mode_label_metrics.advanceWidth);
                    }
                    // popup padding (1 each side) + delegate padding (6 each side)
                    widest_label_width = Math.ceil(w) + 14;
                }

                onModelChanged: recompute_widest_label_width()

                // Clamped against Overlay.overlay, never against the control or
                // a declaring item — a StackLayout gives its non-current
                // children a size of 0, which is the trap that collapsed the
                // Gloss dialogs (see docs/android-edge-to-edge-and-safe-areas.md).
                // Qualified through the id on purpose: unqualified inside
                // Binding (a QObject, not an Item) the attached overlay is null.
                readonly property int target_popup_width: {
                    const wanted = Math.max(search_mode_dropdown.width,
                                            search_mode_dropdown.widest_label_width);
                    const ov = search_mode_dropdown.Overlay.overlay;
                    if (ov && ov.width > 20) {
                        return Math.min(wanted, ov.width - 20);
                    }
                    return wanted;
                }

                // Note: reading `popup` un-defers it — QQuickComboBox::popup()
                // calls executePopup() when it has not been built yet
                // (qquickcombobox.cpp:1371-1377) — so this creates the drop-down
                // at startup instead of at first open. Accepted knowingly: it is
                // one small Popup over a 5-item list, nothing like the costs
                // docs/startup-sequence-and-caches.md §6 is about. If a startup
                // trace ever implicates it, set the width from popup.onAboutToShow
                // instead of binding it.
                Binding {
                    target: search_mode_dropdown.popup
                    property: "width"
                    value: search_mode_dropdown.target_popup_width
                }
            }

            // Button {
            //     id: language_include_btn
            //     checkable: true
            //     icon.source: "icons/32x32/fa_plus-solid.png"
            //     Layout.preferredHeight: root.icon_size
            //     Layout.preferredWidth: root.icon_size
            //     ToolTip.visible: hovered
            //     ToolTip.text: "+ means 'must include', - means 'must exclude'"
            // }

            ComboBox {
                id: language_filter_dropdown
                Layout.preferredHeight: root.icon_size
                Layout.preferredWidth: root.is_wide ? 120 : 80
                // The model is rebuilt per area by load_language_labels_for_area;
                // index 0 ("Language"/"Lang") is the no-filter sentinel.
                model: root.is_wide ? ["Language",] : ["Lang",]
                enabled: root.search_area === "Suttas" || root.search_area === "Library" || root.search_area === "Dictionary"

                // When true, suppress side-effects (persistence + query) of
                // currentIndex changes caused by programmatic restores rather
                // than by the user. Mirrors search_mode_dropdown.
                property bool suppress_persist: false

                // Tracks the area whose saved language is currently applied, so
                // a currentIndex change that arrives mid-area-switch is not
                // mistaken for a user choice.
                property string applied_area: ""

                // The language key this dropdown last applied, by restore or by
                // user choice. The no-op guard in onCurrentIndexChanged compares
                // against this and not against the saved key, which is
                // process-global and can have been changed by another window.
                property string applied_key: "Language"

                // Rebuild the model for the current area and restore the
                // per-area saved language key (defaulting to index 0 = no
                // filter). The language key is persisted separately per area,
                // exactly like the search mode (see set_language_filter_key).
                function restore_for_current_area() {
                    apply_labels_and_key(SuttaBridge.get_language_filter_key(root.search_area));
                }

                // Rebuild the model for the current area and select `key`, or
                // index 0 (no filter) when the key is not among the labels.
                //
                // The model assignment must be inside suppress_persist: it
                // resets currentIndex to 0 on the spot (QQuickComboBox::setModel),
                // and on a width relabel applied_area already matches, so that
                // reset would otherwise be saved as "Language" and fire an
                // unfiltered query while the dropdown goes on showing `key`.
                function apply_labels_and_key(key: string) {
                    suppress_persist = true;
                    root.load_language_labels_for_area(root.search_area);
                    let idx = 0;
                    if (key && key !== "Language" && key !== "Lang") {
                        const found = model.indexOf(key);
                        if (found !== -1) idx = found;
                    }
                    currentIndex = idx;
                    suppress_persist = false;
                    applied_area = root.search_area;
                    applied_key = get_text();
                }

                // The language key a query must run with. Same rule as
                // search_mode_dropdown.mode_for_query(): the saved key until
                // this area has been restored (a fallback — unlike the restore,
                // it does not check the key is still among the labels), this
                // window's own selection after.
                function language_for_query(): string {
                    if (applied_area !== root.search_area) {
                        const saved = SuttaBridge.get_language_filter_key(root.search_area);
                        return saved ? saved : "Language";
                    }
                    return get_text();
                }

                Component.onCompleted: root.ensure_dropdowns_restored()

                Connections {
                    target: root
                    // Area change: rebuild labels + restore the saved language
                    // for the new area. Pure restore — the single query is
                    // fired afterwards by root's area_query_coordinator.
                    function onSearch_areaChanged() {
                        root.ensure_dropdowns_restored();
                    }
                    // is_wide toggles the first-label width ("Language"↔"Lang")
                    // and rebuilds the model. Keeps this window's own selection
                    // rather than re-reading the saved key, which another window
                    // may have changed. No query is fired (this is only a
                    // relabel).
                    function onIs_wideChanged() {
                        if (language_filter_dropdown.applied_area !== root.search_area) {
                            language_filter_dropdown.restore_for_current_area();
                        } else {
                            language_filter_dropdown.apply_labels_and_key(language_filter_dropdown.get_text());
                        }
                    }
                }

                onCurrentIndexChanged: {
                    if (suppress_persist) return;
                    if (!enabled) return;
                    // Mid-transition between search areas: the model just
                    // rebound and ComboBox auto-clipped currentIndex before the
                    // area-restore could run. Ignore — restore_for_current_area
                    // will set the correct index for the new area.
                    if (applied_area !== root.search_area) return;
                    // No-op guard: if the value is the one this dropdown already
                    // applied, skip — otherwise a re-asserted index would fire a
                    // redundant query on top of the area_query_coordinator.
                    const new_key = get_text();
                    if (new_key === applied_key) return;
                    applied_key = new_key;
                    SuttaBridge.set_language_filter_key(root.search_area, new_key);
                    // Re-run search (handle_query will check text min length)
                    root.handle_query_fn(search_input.text); // qmllint disable use-proper-function
                }

                function get_text(): string {
                    // Always return "Language" for index 0, because it is a fixed keyword for
                    // "no language filter is selected".
                    if (currentIndex <= 0) {
                        return "Language";
                    } else {
                        return model[currentIndex];
                    }
                }
            }

            // Info button: opens the Search Help window (explains the search
            // modes and input behaviour). Placed after the language dropdown.
            Button {
                id: search_help_btn
                icon.source: "icons/32x32/fa_circle-info-solid.png"
                flat: true
                Layout.preferredHeight: root.icon_size
                Layout.preferredWidth: root.icon_size
                ToolTip.visible: hovered
                ToolTip.text: "Search help"
                onClicked: root.helpRequested()
            }

            // Fires the single per-area-switch query; the dropdowns' own
            // search_areaChanged handlers only restore. The order these
            // handlers run in is not guaranteed, and in practice this one runs
            // first — hence ensure_dropdowns_restored() before the query.
            Connections {
                id: area_query_coordinator
                target: root
                function onSearch_areaChanged() {
                    root.ensure_dropdowns_restored();
                    root.handle_query_fn(search_input.text); // qmllint disable use-proper-function
                }
            }

            // Button {
            //     id: source_include_btn
            //     checkable: true
            //     icon.source: "icons/32x32/fa_plus-solid.png"
            //     Layout.preferredHeight: root.icon_size
            //     Layout.preferredWidth: root.icon_size
            //     ToolTip.visible: hovered
            //     ToolTip.text: "+ means 'must include', - means 'must exclude'"
            // }

            // ComboBox {
            //     id: source_filter_dropdown
            //     Layout.preferredHeight: root.icon_size
            //     model: [
            //         "Sources",
            //         "ms",
            //         "cst",
            //     ]
            // }
        }
    }
}
