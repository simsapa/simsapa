import QtQuick
import QtTest

// Pure-function tests for DeconstructorUtils (PRD FR-B2). Run with the QML test
// harness (`make qml-test`) — do not run automatically; the user runs QML tests.
TestCase {
    id: test_case
    name: "TestDeconstructorUtils"

    DeconstructorUtils { id: dec_utils }

    // Fixture modeled on `sādhūti`: one direct match plus two break-downs that
    // share the `iti` component.
    readonly property var grouped: ({
        query: "sādhūti",
        results: [
            { uid: "direct/dpd" },
            { uid: "sadhu/dpd" },
            { uid: "sadhu2/dpd" },
            { uid: "iti/dpd" }
        ],
        deconstructions: [
            { words_joined: "sādhu + iti", components: [
                { word: "sādhu", result_uids: ["sadhu/dpd"] },
                { word: "iti", result_uids: ["iti/dpd"] }
            ] },
            { words_joined: "sādhū + iti", components: [
                { word: "sādhū", result_uids: ["sadhu2/dpd"] },
                { word: "iti", result_uids: ["iti/dpd"] }
            ] }
        ],
        direct_uids: ["direct/dpd"]
    })

    function test_unlocked_shows_all_uids() {
        let v = dec_utils.visible_uids(test_case.grouped, 0, false);
        compare(v.length, 4, "unlocked should show every result uid");
        verify(v.indexOf("direct/dpd") !== -1);
        verify(v.indexOf("sadhu/dpd") !== -1);
        verify(v.indexOf("sadhu2/dpd") !== -1);
        verify(v.indexOf("iti/dpd") !== -1);
    }

    function test_locked_first_breakdown() {
        let v = dec_utils.visible_uids(test_case.grouped, 0, true);
        // direct ∪ break-down 0 components
        compare(v.length, 3);
        verify(v.indexOf("direct/dpd") !== -1, "direct match always visible");
        verify(v.indexOf("sadhu/dpd") !== -1);
        verify(v.indexOf("iti/dpd") !== -1);
        verify(v.indexOf("sadhu2/dpd") === -1, "other break-down's component hidden");
    }

    function test_locked_second_breakdown() {
        let v = dec_utils.visible_uids(test_case.grouped, 1, true);
        compare(v.length, 3);
        verify(v.indexOf("direct/dpd") !== -1);
        verify(v.indexOf("sadhu2/dpd") !== -1);
        verify(v.indexOf("iti/dpd") !== -1);
        verify(v.indexOf("sadhu/dpd") === -1);
    }

    function test_shared_component_membership() {
        // `iti` belongs to both break-downs.
        let b = dec_utils.breakdowns_of_uid(test_case.grouped, "iti/dpd");
        compare(b.length, 2);
        verify(b.indexOf(0) !== -1);
        verify(b.indexOf(1) !== -1);
    }

    function test_single_breakdown_membership() {
        let b = dec_utils.breakdowns_of_uid(test_case.grouped, "sadhu/dpd");
        compare(b.length, 1);
        compare(b[0], 0);
    }

    function test_direct_uid_has_no_breakdown() {
        let b = dec_utils.breakdowns_of_uid(test_case.grouped, "direct/dpd");
        compare(b.length, 0, "a direct-only uid belongs to no break-down");
    }

    function test_null_grouped_is_safe() {
        compare(dec_utils.visible_uids(null, 0, false).length, 0);
        compare(dec_utils.breakdowns_of_uid(null, "x").length, 0);
    }

    function test_uid_is_visible_helper() {
        let v = dec_utils.visible_uids(test_case.grouped, 0, true);
        verify(dec_utils.uid_is_visible(v, "iti/dpd"));
        verify(!dec_utils.uid_is_visible(v, "sadhu2/dpd"));
    }
}
