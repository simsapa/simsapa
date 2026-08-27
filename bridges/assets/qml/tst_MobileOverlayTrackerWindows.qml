import QtQuick
import QtQuick.Controls
import QtTest

// Regression guard for the tracker's *in-tree child window* walk — the half
// that is not covered by tst_MobileOverlayTracker.qml, which only exercises
// popups.
//
// This half is the fragile one: `collect_windows()` matches windows by duck
// test rather than by type (a Window is not a QQuickItem, so QML cannot type
// check it), walks `resources`/`children`/`data` which have no change
// notification, and drives an Instantiator from a plain JS array of QObjects.
// It works today, so this is a guard against a Qt upgrade moving where a
// declared child Window lands — not a bug hunt.
//
// It lives in its own file because a TestCase is not an ApplicationWindow, so
// the walk needs a real window fixture to start from; that is why the coverage
// was missing, not an oversight to repeat.
//
// See docs/mobile-webview-visibility-management.md.
TestCase {
    id: test_case
    name: "MobileOverlayTrackerWindows"
    when: windowShown

    property var fixture: null

    Component {
        id: fixture_component

        ApplicationWindow {
            id: fixture_root
            width: 300
            height: 200

            property alias tracker: fixture_tracker
            property alias direct_window: direct_child_window
            property alias nested_window: nested_child_window

            MobileOverlayTracker {
                id: fixture_tracker
                is_mobile: true
            }

            // Mirrors the ten in-tree windows in SuttaSearchWindow.qml: an
            // ApplicationWindow with flags: Qt.Dialog declared directly in the
            // tracked window's tree.
            ApplicationWindow {
                id: direct_child_window
                flags: Qt.Dialog
                width: 200
                height: 150
                title: "Direct"
            }

            // Mirrors the gloss_tab.commonWordsDialog shape: a window declared
            // one level deeper, so the walk has to recurse through an
            // intermediate Item to reach it.
            Item {
                id: intermediate_item

                ApplicationWindow {
                    id: nested_child_window
                    flags: Qt.Dialog
                    width: 200
                    height: 150
                    title: "Nested"
                }
            }
        }
    }

    function initTestCase() {
        test_case.fixture = fixture_component.createObject(null);
        verify(test_case.fixture !== null, "window fixture created");
        test_case.fixture.show();
        // The scan is Qt.callLater-deferred, so it has not run yet at this
        // point; every test waits for it via tracked_windows.
        tryVerify(function() { return test_case.fixture.tracker.tracked_windows.length === 2; },
                  2000, "the walk must find both declared child windows");
    }

    function cleanupTestCase() {
        if (test_case.fixture) {
            test_case.fixture.destroy();
            test_case.fixture = null;
        }
    }

    function init() {
        let f = test_case.fixture;
        f.tracker.is_mobile = true;
        f.direct_window.visible = false;
        f.nested_window.visible = false;
        tryCompare(f.tracker, "any_open", false, 2000);
    }

    function test_01_walk_finds_both_declared_child_windows() {
        let f = test_case.fixture;
        let found = f.tracker.tracked_windows;
        compare(found.length, 2, "exactly the two declared child windows");
        verify(found.indexOf(f.direct_window) >= 0,
               "a directly declared child window must be found in contentItem.resources/data");
        verify(found.indexOf(f.nested_window) >= 0,
               "a child window declared one component deeper must be reached by recursing");
    }

    function test_02_direct_child_window_flips_any_open() {
        let f = test_case.fixture;
        compare(f.tracker.any_open, false);
        f.direct_window.visible = true;
        tryCompare(f.tracker, "any_open", true, 2000,
                   "a visible in-tree child window must hide the webview");
        f.direct_window.visible = false;
        tryCompare(f.tracker, "any_open", false, 2000,
                   "hiding it must restore the webview");
    }

    function test_03_nested_child_window_flips_any_open() {
        let f = test_case.fixture;
        f.nested_window.visible = true;
        tryCompare(f.tracker, "any_open", true, 2000,
                   "a child window declared inside a sub-component must be tracked too");
        f.nested_window.visible = false;
        tryCompare(f.tracker, "any_open", false, 2000);
    }

    // any_open is a single boolean, not a count: closing one of two open
    // windows must not restore the reader while the other is still up.
    function test_04_two_visible_windows_resolve_to_one_any_open() {
        let f = test_case.fixture;
        f.direct_window.visible = true;
        f.nested_window.visible = true;
        tryCompare(f.tracker, "any_open", true, 2000);

        f.direct_window.visible = false;
        tryCompare(f.tracker, "any_open", true, 2000,
                   "any_open must stay true while the second window is still visible");

        f.nested_window.visible = false;
        tryCompare(f.tracker, "any_open", false, 2000);
    }

    function test_05_desktop_short_circuits() {
        let f = test_case.fixture;
        f.tracker.is_mobile = false;
        f.direct_window.visible = true;
        tryCompare(f.direct_window, "visible", true, 2000);
        compare(f.tracker.any_open, false,
                "on desktop the tracker must report false even with a child window visible");
        f.direct_window.visible = false;
        f.tracker.is_mobile = true;
        tryCompare(f.tracker, "any_open", false, 2000);
    }
}
