pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// The grouped list of storage locations, shared by every entry point: the
// startup recovery dialog, the "storage unavailable" message, the "nothing
// found" message in Database Validation, and Database Validation's own
// selection.
//
// It always renders the SAME list — every location the app can see, sectioned
// into "Existing app data found" / "Available (no app data found)" / "Not
// usable for the database" — so a user who can see their card in the phone can
// find it somewhere in the list and read why it is or is not offered. What
// differs between entry points is only which groups may be selected.
//
// Rows come from StorageManager.find_storage_candidates_json() (tier 1). The
// tier-2 probe verdicts arrive later and are merged in place through
// apply_probe_verdict(), which may only ever DEMOTE a row to unusable.
//
// See docs/relocated-storage-recovery.md.
Item {
    id: root

    property int font_point_size: 12

    // Colours are derived from the palette rather than hardcoded, because this
    // list is shown by the recovery flow — which runs when the database may be
    // missing, so `ThemeHelper` (which reads the saved theme through
    // `SuttaBridge`) is not available. Qt has already filled the palette from
    // the system light/dark setting by then.
    //
    // Hardcoded light-theme colours here produced white-on-pale-blue selected
    // rows and near-invisible group headings on a dark phone.
    readonly property bool dark_background: palette.window.hsvValue < 0.5

    // Secondary lines (path, figures, reasons): the same colour as the primary
    // text, softened, so contrast tracks the theme instead of fighting it.
    readonly property color secondary_text_color:
        Qt.rgba(palette.text.r, palette.text.g, palette.text.b, 0.75)

    // Warnings have to stay legible on both backgrounds; a single fixed amber
    // cannot.
    readonly property color warning_text_color: root.dark_background ? "#ffb74d" : "#b26500"

    // Group names whose rows the user may select. Startup passes both selectable
    // groups; Database Validation passes only "found"; the message screens pass
    // an empty list, which makes the whole list read-only.
    property var selectable_groups: []

    // A blanket off-switch, independent of the groups: the FR-23 / FR-19
    // message screens show the same list purely as a diagnosis.
    property bool selection_enabled: true

    // Database Validation never offers the location the app is already using:
    // "adopting" it would rewrite the identical path and quit for nothing.
    property bool exclude_recorded: false

    // -1 when nothing is selected.
    property int selected_index: -1
    readonly property bool has_selection: root.selected_index >= 0

    // True while the selected row's tier-2 probe has not yet reported.
    //
    // A row stays selectable while it is being probed — hiding its radio button
    // mid-probe makes the pre-selected single hit look unselected — but the
    // hosting screen MUST disable its confirm button on this, because
    // committing before the verdict writes a storage path the probe may be
    // about to reject.
    //
    // Maintained explicitly at every mutation point rather than as a binding: a
    // JS function reading rows_model is not re-evaluated when a model role
    // changes.
    property bool selection_probe_pending: false

    // Emitted when a selected row is taken away by a tier-2 demotion, so the
    // hosting screen can disable its confirm button.
    signal selection_cleared()

    Logger { id: logger }
    ListModel { id: rows_model }

    readonly property int row_count: rows_model.count

    // The height the rows and section headings actually need. The recovery
    // window and Database Validation give the list all the space they have; the
    // first-run dialog sizes itself to its content, so it caps this instead of
    // reserving a fixed block that a one-row list would leave mostly empty.
    readonly property real content_height: candidates_view.contentHeight

    // Populate from the tier-1 scan JSON. Any previous selection is dropped:
    // Try Again re-scans from scratch and the row indices are not comparable.
    function load(candidates_json: string) {
        rows_model.clear();
        root.selected_index = -1;
        root.selection_probe_pending = false;

        var rows = [];
        try {
            rows = JSON.parse(candidates_json);
        } catch (e) {
            logger.error("StorageCandidatesList: cannot parse candidates JSON: " + e
                         + " json: " + candidates_json);
            return;
        }

        for (var i = 0; i < rows.length; i++) {
            var r = rows[i];
            rows_model.append({
                path: r.path === undefined || r.path === null ? "" : r.path,
                label: r.label === undefined || r.label === null ? "Storage" : r.label,
                is_internal: r.is_internal === true,
                is_recorded: r.is_recorded === true,
                group: r.group === undefined || r.group === null ? "unusable" : r.group,
                unusable_reason: r.unusable_reason === undefined || r.unusable_reason === null
                    ? "" : r.unusable_reason,
                // Unmeasured figures are absent from the JSON, never zero — a
                // fabricated 0 renders as "0.0 GB free" and reads as a full
                // volume. -1 is this model's "not measured".
                megabytes_available: r.megabytes_available === undefined
                    || r.megabytes_available === null ? -1 : r.megabytes_available,
                // The volume's size, so the figure can read "12 GB free of
                // 64 GB" — the difference between a nearly empty card and a
                // nearly full one. -1 is "not measured"; the line then falls
                // back to the free-space figure alone.
                megabytes_total: r.megabytes_total === undefined
                    || r.megabytes_total === null ? -1 : r.megabytes_total,
                low_space_warning: r.low_space_warning === true,
                appdata_bytes: r.appdata_bytes === undefined || r.appdata_bytes === null
                    ? -1 : r.appdata_bytes,
                modified: r.modified === undefined || r.modified === null ? "" : r.modified,
                is_complete: r.is_complete === true,
                // Tier-2 state, owned by this component.
                probe_pending: false,
            });
        }

        logger.info("StorageCandidatesList: loaded " + rows_model.count + " candidate(s)");
    }

    function row_at(index: int): var {
        if (index < 0 || index >= rows_model.count) return null;
        return rows_model.get(index);
    }

    function selected_row(): var {
        return root.row_at(root.selected_index);
    }

    // How many rows hold an existing installation. The startup recovery flow
    // branches on this ("hits"), so it must count rows, not selectable rows.
    function found_count(): int {
        var n = 0;
        for (var i = 0; i < rows_model.count; i++) {
            if (rows_model.get(i).group === "found") n++;
        }
        return n;
    }

    // Hits that are something OTHER than the location already in use.
    //
    // This is Database Validation's branch condition, and it is not the same
    // question as found_count(): on a healthy install the recorded path is
    // itself a `found` row, so found_count() is ≥ 1 with nothing to adopt.
    // Branching on found_count() there would show a selection screen on which
    // no row can be picked, instead of the "no database was found on the other
    // storage locations" message.
    function found_count_excluding_recorded(): int {
        var n = 0;
        for (var i = 0; i < rows_model.count; i++) {
            var r = rows_model.get(i);
            if (r.group === "found" && !r.is_recorded) n++;
        }
        return n;
    }

    // Rows the user may act on, given this entry point's rules.
    //
    // A pending tier-2 probe does NOT make a row unselectable: the probe is a
    // demote-only refinement of an already-valid tier-1 verdict, and taking the
    // radio button away from a row the user is looking at (including the
    // pre-selected single hit) reads as a bug. What a pending probe does block
    // is *confirming* — see selection_probe_pending.
    function is_selectable(index: int): bool {
        var r = root.row_at(index);
        if (r === null) return false;
        if (!root.selection_enabled) return false;
        if (root.selectable_groups.indexOf(r.group) < 0) return false;
        if (root.exclude_recorded && r.is_recorded) return false;
        return true;
    }

    function select(index: int) {
        if (!root.is_selectable(index)) return;
        root.selected_index = index;
        root.refresh_selection_probe_pending();
    }

    // Recompute whether the selected row is still waiting on its probe. Called
    // from every place that changes either the selection or a row's pending
    // flag.
    function refresh_selection_probe_pending() {
        var r = root.row_at(root.selected_index);
        root.selection_probe_pending = (r !== null && r.probe_pending === true);
    }

    // Pre-select the single hit, per FR-11. Does nothing when there is more than
    // one, so the user is never nudged towards an arbitrary copy.
    function preselect_single_hit() {
        var hit = -1;
        for (var i = 0; i < rows_model.count; i++) {
            if (rows_model.get(i).group === "found") {
                if (hit >= 0) return; // more than one hit
                hit = i;
            }
        }
        if (hit >= 0 && root.is_selectable(hit)) {
            root.selected_index = hit;
            root.refresh_selection_probe_pending();
        }
    }

    // The first row the user could pick, in the list's own order — which is
    // group order with the internal location first, so this is "the internal
    // location" wherever one is selectable.
    function first_selectable_index(): int {
        for (var i = 0; i < rows_model.count; i++) {
            if (root.is_selectable(i)) return i;
        }
        return -1;
    }

    function first_selectable_row(): var {
        return root.row_at(root.first_selectable_index());
    }

    // Start with a sensible default rather than a dialog whose confirm button
    // is dead until something is touched. Used by the first-run destination
    // picker, where any usable location is a legitimate choice; the recovery
    // flow uses preselect_single_hit() instead, because there a default would
    // nudge the user towards an arbitrary copy of their data.
    function preselect_first_selectable() {
        var i = root.first_selectable_index();
        if (i < 0) return;
        root.selected_index = i;
        root.refresh_selection_probe_pending();
    }

    // How many rows the user could actually pick, under this entry point's
    // rules. Not row_count (which includes unusable rows) and not found_count():
    // the first-run dialog auto-selects when there is exactly ONE choice, and
    // counting rows the user cannot pick would turn that into a modal offering a
    // single option.
    function selectable_count(): int {
        var n = 0;
        for (var i = 0; i < rows_model.count; i++) {
            if (root.is_selectable(i)) n++;
        }
        return n;
    }

    // Paths worth probing.
    //
    // A probe writes a throwaway SQLite database into the candidate directory,
    // so it is only ever run where its verdict can change what the user is
    // allowed to do. `selectable_only` is what Database Validation needs: there
    // the `available` group and the recorded path's own row are shown but not
    // selectable, and probing them would touch volumes to produce a demotion
    // nobody can act on. The startup dialog passes false (both groups are
    // selectable there, and its own confirm button is what a verdict gates).
    //
    // The message screens never call this at all — nothing can be selected on
    // them, so nothing is probed.
    function probeable_paths(selectable_only: bool): var {
        var paths = [];
        for (var i = 0; i < rows_model.count; i++) {
            var r = rows_model.get(i);
            if (r.group === "unusable") continue;
            if (selectable_only && !root.is_selectable(i)) continue;
            paths.push(r.path);
        }
        return paths;
    }

    function set_probe_pending(path: string, pending: bool) {
        var i = root.index_of_path(path);
        if (i < 0) return;
        rows_model.setProperty(i, "probe_pending", pending);
        root.refresh_selection_probe_pending();
    }

    // Merge one tier-2 verdict. Demote-only: a probe can move a row to
    // "unusable", never promote one back, because tier 1 already established
    // everything a probe cannot see.
    function apply_probe_verdict(path: string, is_usable: bool, reason: string) {
        var i = root.index_of_path(path);
        if (i < 0) {
            logger.info("StorageCandidatesList: probe verdict for an unknown row: " + path);
            return;
        }

        rows_model.setProperty(i, "probe_pending", false);

        if (is_usable) {
            root.refresh_selection_probe_pending();
            return;
        }

        logger.info("StorageCandidatesList: demoting " + path + " — " + reason);
        rows_model.setProperty(i, "group", "unusable");
        rows_model.setProperty(i, "unusable_reason",
                               reason === "" ? "Not usable for the database" : reason);
        // Figures are omitted on unusable rows.
        rows_model.setProperty(i, "megabytes_available", -1);
        rows_model.setProperty(i, "megabytes_total", -1);
        rows_model.setProperty(i, "low_space_warning", false);
        rows_model.setProperty(i, "appdata_bytes", -1);
        rows_model.setProperty(i, "modified", "");

        if (root.selected_index === i) {
            root.selected_index = -1;
            root.selection_probe_pending = false;
            root.selection_cleared();
        }

        // Move the demoted row to the end of the model.
        //
        // The ListView's section headings come from row ORDER — the scan hands
        // the rows over already grouped — so a row whose group changes in place
        // splits its section: a demoted row sitting inside the "found" run
        // renders a second "Not usable for the database" heading mid-list, with
        // the remaining found rows underneath it. The move keeps the model in
        // group order, which is the invariant the sections rely on.
        var last = rows_model.count - 1;
        if (i < last) {
            rows_model.move(i, last, 1);
            // Every row after i shifted down by one.
            if (root.selected_index > i) {
                root.selected_index -= 1;
            }
        }

        root.refresh_selection_probe_pending();
    }

    // Normalized comparison, matching the Rust scan's same_path(): trim and drop
    // trailing separators. A raw compare silently fails on a trailing slash.
    function index_of_path(path: string): int {
        var wanted = root.normalize_path(path);
        for (var i = 0; i < rows_model.count; i++) {
            if (root.normalize_path(rows_model.get(i).path) === wanted) return i;
        }
        return -1;
    }

    function normalize_path(path: string): string {
        var p = ("" + path).trim();
        while (p.length > 1 && p.charAt(p.length - 1) === "/") {
            p = p.substring(0, p.length - 1);
        }
        return p;
    }

    function group_heading(group: string): string {
        if (group === "found") return "Existing app data found";
        if (group === "available") return "Available (no app data found)";
        return "Not usable for the database";
    }

    function megabytes_to_gb(megabytes: int): string {
        return (megabytes / 1024).toFixed(1);
    }

    function bytes_to_mb(bytes: real): string {
        return (bytes / 1024 / 1024).toFixed(0);
    }

    ListView {
        id: candidates_view
        anchors.fill: parent
        model: rows_model
        clip: true
        spacing: 6

        // The scan returns rows already in group order, so sections need no
        // sorting of their own.
        section.property: "group"
        section.criteria: ViewSection.FullString
        section.delegate: Label {
            required property string section
            width: candidates_view.width
            topPadding: 8
            bottomPadding: 2
            text: root.group_heading(section)
            font.pointSize: root.font_point_size - 1
            font.bold: true
            color: palette.text
            wrapMode: Text.WordWrap
        }

        delegate: ItemDelegate {
            id: row_item

            width: candidates_view.width
            height: row_column.implicitHeight + 16
            // Unusable rows are visually de-emphasised and inert, in the same
            // delegate as the rest, so the list reads as one sectioned list of
            // "what the app can see".
            //
            // Computed from the delegate's own required properties, not from
            // root.is_selectable(index): a function call is not re-evaluated
            // when a model role changes, so a tier-2 demotion would leave the
            // row still clickable. The required properties ARE reactive.
            //
            // `probe_pending` is deliberately NOT part of this: a row being
            // probed stays selectable and keeps its radio button (the mirror of
            // is_selectable()). Blocking the *confirmation* is the host's job,
            // through root.selection_probe_pending.
            readonly property bool row_selectable:
                root.selection_enabled
                && root.selectable_groups.indexOf(row_item.group) >= 0
                && !(root.exclude_recorded && row_item.is_recorded)

            enabled: row_item.row_selectable
            opacity: row_item.group === "unusable" ? 0.7 : 1.0

            required property int index
            required property string path
            required property string label
            required property bool is_internal
            required property bool is_recorded
            required property string group
            required property string unusable_reason
            required property int megabytes_available
            required property int megabytes_total
            required property bool low_space_warning
            required property real appdata_bytes
            required property string modified
            required property bool is_complete
            required property bool probe_pending

            onClicked: root.select(row_item.index)

            Rectangle {
                anchors.fill: parent
                radius: 5
                border.width: 1
                border.color: root.selected_index === row_item.index
                    ? palette.highlight
                    : Qt.rgba(palette.text.r, palette.text.g, palette.text.b, 0.25)
                // A tint rather than a fill: the row's text keeps palette.text,
                // which a solid light-blue background made unreadable in dark mode.
                color: root.selected_index === row_item.index
                    ? Qt.rgba(palette.highlight.r, palette.highlight.g,
                              palette.highlight.b, 0.30)
                    : "transparent"

                RowLayout {
                    anchors.fill: parent
                    anchors.margins: 6
                    spacing: 6

                    RadioButton {
                        visible: row_item.row_selectable
                        checked: root.selected_index === row_item.index
                        onClicked: root.select(row_item.index)
                        Layout.alignment: Qt.AlignVCenter
                    }

                    ColumnLayout {
                        id: row_column
                        Layout.fillWidth: true
                        spacing: 1

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 6

                            Label {
                                text: row_item.label
                                font.pointSize: root.font_point_size
                                font.bold: true
                                elide: Text.ElideRight
                                Layout.fillWidth: true
                            }

                            // FR-11a: a suffix rendered by the delegate, never
                            // baked into the label the enumeration produced.
                            Label {
                                visible: row_item.is_recorded
                                text: "(current selection)"
                                font.pointSize: root.font_point_size - 3
                                color: root.secondary_text_color
                            }
                        }

                        // The label is often a guess on Android; the path is the
                        // real discriminator between two external candidates.
                        Label {
                            text: row_item.path
                            font.pointSize: root.font_point_size - 3
                            color: root.secondary_text_color
                            elide: Text.ElideMiddle
                            Layout.fillWidth: true
                        }

                        // Group 1 shows the database size and its last-modified
                        // time — the single most useful field for telling two
                        // copies apart. Group 2 shows free space. "Database
                        // size" and "free space" are different quantities and
                        // are never collapsed into a bare "size".
                        Label {
                            visible: row_item.group === "found" && row_item.appdata_bytes >= 0
                            text: "Database " + root.bytes_to_mb(row_item.appdata_bytes) + " MB"
                                + (row_item.modified === "" ? "" : " · " + row_item.modified)
                            font.pointSize: root.font_point_size - 2
                            color: palette.text
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        // Free space is shown on every usable row, not only on
                        // group 2: a `found` row can be picked as a download
                        // destination at first run, and FR-30 requires the
                        // low-space warning to come with the figure it is about.
                        Label {
                            visible: row_item.group !== "unusable"
                                && row_item.megabytes_available >= 0
                            text: root.megabytes_to_gb(row_item.megabytes_available) + " GB free"
                                + (row_item.megabytes_total >= 0
                                   ? " of " + root.megabytes_to_gb(row_item.megabytes_total) + " GB"
                                   : "")
                            font.pointSize: root.font_point_size - 2
                            color: palette.text
                            Layout.fillWidth: true
                        }

                        // At most one short status marker per row.
                        Label {
                            visible: row_item.group === "found" && !row_item.is_complete
                            text: "Partial — some databases are missing"
                            font.pointSize: root.font_point_size - 2
                            color: root.warning_text_color
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        // A warning, never a disqualification: the required size
                        // is not knowable here.
                        Label {
                            visible: row_item.group !== "unusable" && row_item.low_space_warning
                            text: "May not have enough free space"
                            font.pointSize: root.font_point_size - 2
                            color: root.warning_text_color
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Label {
                            visible: row_item.group === "unusable"
                            text: row_item.unusable_reason
                            font.pointSize: root.font_point_size - 2
                            color: root.secondary_text_color
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        // The list renders immediately from tier 1; a slow or
                        // half-mounted volume shows this instead of stalling it.
                        Label {
                            visible: row_item.probe_pending
                            text: "Checking…"
                            font.pointSize: root.font_point_size - 2
                            color: root.secondary_text_color
                            Layout.fillWidth: true
                        }
                    }
                }
            }
        }
    }
}
