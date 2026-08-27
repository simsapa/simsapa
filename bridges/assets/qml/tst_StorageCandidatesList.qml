import QtQuick
import QtTest

// The rules that decide what the user can pick, and what a tier-2 probe verdict
// is allowed to do to a row. These are the parts of the recovery UI that are
// easy to get subtly wrong and impossible to notice by looking at the screen.
//
// See docs/relocated-storage-recovery.md.
TestCase {
    id: test_case
    width: 500
    height: 600
    visible: true
    when: windowShown
    name: "TestStorageCandidatesList"

    // Two hits (one of them the recorded path), one available location and one
    // unusable volume — the shape the recovery dialog is built for.
    readonly property string sample_json: JSON.stringify([
        {
            path: "/data/user/0/app/files", label: "Internal Storage",
            is_internal: true, is_recorded: false, group: "found",
            unusable_reason: "", megabytes_available: 100000,
            low_space_warning: false, appdata_bytes: 512000000,
            modified: "2026-08-01 10:00", is_complete: true
        },
        {
            path: "/storage/ABCD-1234/Android/data/app/files", label: "SD Card",
            is_internal: false, is_recorded: true, group: "found",
            unusable_reason: "", megabytes_available: 30000,
            low_space_warning: false, appdata_bytes: 512000000,
            modified: "2026-07-20 09:00", is_complete: false
        },
        {
            path: "/storage/EEEE-5678/Android/data/app/files", label: "USB Storage",
            is_internal: false, is_recorded: false, group: "available",
            unusable_reason: "", megabytes_available: 8000,
            low_space_warning: false, appdata_bytes: null,
            modified: null, is_complete: null
        },
        {
            path: "/storage/FFFF-9999", label: "Card Reader",
            is_internal: false, is_recorded: false, group: "unusable",
            unusable_reason: "Not usable for app data",
            megabytes_available: null, low_space_warning: false,
            appdata_bytes: null, modified: null, is_complete: null
        }
    ])

    StorageCandidatesList {
        id: candidates
        anchors.fill: parent
        selectable_groups: ["found", "available"]
    }

    SignalSpy {
        id: selection_cleared_spy
        target: candidates
        signalName: "selection_cleared"
    }

    function init() {
        selection_cleared_spy.clear();
        candidates.selection_enabled = true;
        candidates.exclude_recorded = false;
        candidates.selectable_groups = ["found", "available"];
        candidates.load(test_case.sample_json);
    }

    function test_loads_every_candidate_not_only_the_hits() {
        // The list is a picture of the whole device: a user who can see their
        // card in the phone must find it somewhere in the list.
        compare(candidates.row_count, 4);
        compare(candidates.found_count(), 2);
    }

    function test_unusable_rows_are_never_selectable() {
        verify(!candidates.is_selectable(3));
        candidates.select(3);
        compare(candidates.selected_index, -1);
    }

    function test_startup_may_select_both_groups() {
        verify(candidates.is_selectable(0));
        verify(candidates.is_selectable(2));
    }

    function test_database_validation_excludes_group_2_and_the_current_selection() {
        // Adopting the location the app is already using would rewrite the same
        // path and quit the whole app for nothing; "download here instead" is
        // not a meaningful answer while the app is running.
        candidates.selectable_groups = ["found"];
        candidates.exclude_recorded = true;

        verify(candidates.is_selectable(0));
        verify(!candidates.is_selectable(1), "the recorded row must not be selectable");
        verify(!candidates.is_selectable(2), "group 2 must not be selectable here");
    }

    function test_read_only_mode_disables_everything() {
        candidates.selection_enabled = false;
        for (var i = 0; i < candidates.row_count; i++) {
            verify(!candidates.is_selectable(i));
        }
    }

    function test_single_hit_is_preselected() {
        candidates.load(JSON.stringify([
            {
                path: "/data/user/0/app/files", label: "Internal Storage",
                is_internal: true, is_recorded: false, group: "found",
                unusable_reason: "", megabytes_available: 100000,
                low_space_warning: false, appdata_bytes: 1024,
                modified: "2026-08-01 10:00", is_complete: true
            }
        ]));
        candidates.preselect_single_hit();
        compare(candidates.selected_index, 0);
    }

    function test_two_hits_are_not_preselected() {
        // With more than one copy the user must not be nudged towards an
        // arbitrary one.
        candidates.preselect_single_hit();
        compare(candidates.selected_index, -1);
    }

    function test_probe_demotes_a_row_and_drops_its_figures() {
        candidates.apply_probe_verdict("/storage/EEEE-5678/Android/data/app/files",
                                       false, "The app cannot write here");

        // Demoted rows move to the end — see
        // test_a_demoted_row_moves_into_the_unusable_group.
        var i = candidates.row_count - 1;
        var row = candidates.row_at(i);
        compare(row.path, "/storage/EEEE-5678/Android/data/app/files");
        compare(row.group, "unusable");
        compare(row.unusable_reason, "The app cannot write here");
        compare(row.megabytes_available, -1);
        verify(!candidates.is_selectable(i));
    }

    function test_a_demoted_row_moves_into_the_unusable_group() {
        // The ListView's section headings come from row order, so a row demoted
        // in place would split the "found" section with a stray "not usable"
        // heading and leave the remaining found rows under it.
        candidates.apply_probe_verdict("/data/user/0/app/files",
                                       false, "The app cannot write here");

        compare(candidates.row_at(candidates.row_count - 1).path, "/data/user/0/app/files");

        // Everything before it is still in group order.
        var seen_non_found = false;
        for (var i = 0; i < candidates.row_count; i++) {
            var g = candidates.row_at(i).group;
            if (g !== "found") seen_non_found = true;
            else verify(!seen_non_found, "a found row must not follow a non-found row");
        }
    }

    function test_demotion_keeps_a_selection_made_on_a_later_row() {
        // Moving the demoted row shifts every row after it down by one; a
        // selection tracked by index has to move with it.
        candidates.select(2);
        candidates.apply_probe_verdict("/data/user/0/app/files",
                                       false, "The app cannot write here");

        compare(candidates.selected_row().path, "/storage/EEEE-5678/Android/data/app/files");
        compare(selection_cleared_spy.count, 0);
    }

    function test_probe_never_promotes_a_row() {
        candidates.apply_probe_verdict("/storage/FFFF-9999", true, "");
        compare(candidates.row_at(3).group, "unusable");
    }

    function test_demoting_the_selected_row_clears_the_selection() {
        candidates.select(2);
        compare(candidates.selected_index, 2);

        candidates.apply_probe_verdict("/storage/EEEE-5678/Android/data/app/files",
                                       false, "This location cannot store the app database");

        compare(candidates.selected_index, -1);
        compare(selection_cleared_spy.count, 1);
    }

    function test_probe_verdicts_match_paths_with_a_trailing_slash() {
        // A raw string compare silently fails on a trailing slash, leaving the
        // row pending forever with no error anywhere.
        candidates.apply_probe_verdict("/storage/EEEE-5678/Android/data/app/files/",
                                       false, "The app cannot write here");
        var row = candidates.row_at(candidates.row_count - 1);
        compare(row.path, "/storage/EEEE-5678/Android/data/app/files");
        compare(row.group, "unusable");
    }

    function test_probeable_paths_exclude_unusable_rows() {
        var paths = candidates.probeable_paths(false);
        compare(paths.length, 3);
        compare(paths.indexOf("/storage/FFFF-9999"), -1);
    }

    function test_probeable_paths_can_be_limited_to_selectable_rows() {
        // A probe writes a file into the candidate directory, so Database
        // Validation — where group 2 and the recorded row are shown but cannot
        // be picked — must not touch them for a verdict nobody can act on.
        candidates.selectable_groups = ["found"];
        candidates.exclude_recorded = true;

        var paths = candidates.probeable_paths(true);
        compare(paths.length, 1);
        compare(paths[0], "/data/user/0/app/files");

        // The startup dialog, where both groups are selectable, still probes
        // every non-unusable row.
        candidates.selectable_groups = ["found", "available"];
        candidates.exclude_recorded = false;
        compare(candidates.probeable_paths(true).length, 3);
    }

    function test_selectable_count_follows_the_entry_point_rules() {
        // The first-run dialog auto-selects when there is exactly one choice;
        // counting rows the user cannot pick would turn that into a modal
        // offering a single option.
        compare(candidates.selectable_count(), 3);

        candidates.selectable_groups = ["found"];
        candidates.exclude_recorded = true;
        compare(candidates.selectable_count(), 1);

        candidates.selection_enabled = false;
        compare(candidates.selectable_count(), 0);
    }

    function test_a_pending_row_stays_selectable() {
        // A probe is a demote-only refinement of a verdict tier 1 already made,
        // so the row keeps its radio button while it runs. Taking selection away
        // mid-probe made the pre-selected single hit look unselected.
        candidates.set_probe_pending("/data/user/0/app/files", true);
        verify(candidates.is_selectable(0));
        candidates.select(0);
        compare(candidates.selected_index, 0);
    }

    function test_a_pending_probe_on_the_selected_row_blocks_confirmation() {
        // What a pending probe blocks is committing: the host's confirm button
        // binds to this.
        candidates.select(0);
        verify(!candidates.selection_probe_pending);

        candidates.set_probe_pending("/data/user/0/app/files", true);
        verify(candidates.selection_probe_pending, "the selected row is being probed");

        // A probe on some other row is not the user's problem.
        candidates.set_probe_pending("/data/user/0/app/files", false);
        candidates.set_probe_pending("/storage/EEEE-5678/Android/data/app/files", true);
        verify(!candidates.selection_probe_pending);
    }

    function test_a_usable_verdict_clears_the_pending_block() {
        candidates.select(0);
        candidates.set_probe_pending("/data/user/0/app/files", true);
        candidates.apply_probe_verdict("/data/user/0/app/files", true, "");
        verify(!candidates.selection_probe_pending);
        compare(candidates.selected_index, 0);
    }

    function test_selecting_a_row_recomputes_the_pending_block() {
        candidates.set_probe_pending("/storage/EEEE-5678/Android/data/app/files", true);
        candidates.select(2);
        verify(candidates.selection_probe_pending);
    }

    function test_found_count_excluding_recorded_ignores_the_current_location() {
        // Database Validation's branch condition: on a healthy install the
        // recorded path is itself a hit, and there is nothing to adopt.
        compare(candidates.found_count(), 2);
        compare(candidates.found_count_excluding_recorded(), 1);

        candidates.load(JSON.stringify([
            {
                path: "/storage/ABCD-1234/Android/data/app/files", label: "SD Card",
                is_internal: false, is_recorded: true, group: "found",
                unusable_reason: "", megabytes_available: 30000,
                low_space_warning: false, appdata_bytes: 512000000,
                modified: "2026-07-20 09:00", is_complete: true
            }
        ]));
        compare(candidates.found_count(), 1);
        compare(candidates.found_count_excluding_recorded(), 0);
    }

    function test_first_selectable_is_the_internal_location() {
        // The first-run dialog opens with a default selection, and the scan
        // orders the internal location first within its group.
        compare(candidates.first_selectable_index(), 0);
        candidates.preselect_first_selectable();
        compare(candidates.selected_index, 0);
        compare(candidates.first_selectable_row().path, "/data/user/0/app/files");
    }

    function test_first_selectable_skips_rows_this_entry_point_cannot_pick() {
        // Database Validation's rules: the recorded path is not an adoption
        // candidate and group 2 is not selectable, so the first pickable row is
        // neither row 0's group-mate nor the available one.
        candidates.exclude_recorded = true;
        candidates.selectable_groups = ["found"];
        candidates.load(JSON.stringify([
            {
                path: "/storage/ABCD-1234/Android/data/app/files", label: "SD Card",
                is_internal: false, is_recorded: true, group: "found",
                unusable_reason: "", megabytes_available: 30000,
                low_space_warning: false, appdata_bytes: 512000000,
                modified: "2026-07-20 09:00", is_complete: true
            },
            {
                path: "/storage/EEEE-5678/Android/data/app/files", label: "USB Storage",
                is_internal: false, is_recorded: false, group: "found",
                unusable_reason: "", megabytes_available: 8000,
                low_space_warning: false, appdata_bytes: 512000000,
                modified: "2026-07-01 09:00", is_complete: true
            }
        ]));

        compare(candidates.first_selectable_index(), 1);
        candidates.preselect_first_selectable();
        compare(candidates.selected_index, 1);

        candidates.exclude_recorded = false;
        candidates.selectable_groups = ["found", "available"];
        candidates.load(test_case.sample_json);
    }

    function test_preselect_first_selectable_does_nothing_with_no_pickable_row() {
        // The read-only message screens: nothing may be selected, so nothing is.
        candidates.selection_enabled = false;
        candidates.load(test_case.sample_json);
        candidates.preselect_first_selectable();
        compare(candidates.selected_index, -1);
        compare(candidates.selectable_count(), 0);

        candidates.selection_enabled = true;
        candidates.load(test_case.sample_json);
    }

    function test_loading_again_drops_the_previous_selection() {
        // Try Again re-scans from scratch; row indices are not comparable.
        candidates.select(0);
        candidates.load(test_case.sample_json);
        compare(candidates.selected_index, -1);
    }
}
