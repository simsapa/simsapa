import QtQuick
import QtQuick.Controls
import QtTest

// The tracker decides whether the mobile HTML reader is hidden, and every
// behaviour it has is short-circuited away on desktop — so these tests force
// the mobile branch with `tracker.is_mobile = true`.
//
// See docs/mobile-webview-visibility-management.md.
TestCase {
    id: test_case
    name: "MobileOverlayTracker"
    width: 400
    height: 400
    visible: true
    when: windowShown

    MobileOverlayTracker {
        id: tracker
        is_mobile: true
    }

    // Declared but never opened: must not count.
    Dialog {
        id: closed_dialog
        anchors.centerIn: parent
        title: "Closed"
    }

    Dialog {
        id: test_dialog
        anchors.centerIn: parent
        title: "Test"
    }

    Dialog {
        id: modal_dialog
        anchors.centerIn: parent
        title: "Modal"
        modal: true
    }

    Menu {
        id: test_menu
        MenuItem { text: "One" }
        MenuItem { text: "Two" }
    }

    Component {
        id: dynamic_dialog_component
        Dialog {
            anchors.centerIn: parent
            title: "Dynamic"
        }
    }

    Item {
        id: tooltip_host
        width: 50
        height: 20
    }

    readonly property int overlay_child_count: tracker.Overlay.overlay
                                               ? tracker.Overlay.overlay.children.length
                                               : -1

    function init() {
        tracker.is_mobile = true;
        tryCompare(tracker, "any_open", false, 2000);
    }

    function test_01_false_at_rest_with_a_declared_closed_dialog() {
        compare(closed_dialog.visible, false);
        compare(tracker.any_open, false,
                "a declared but unopened Dialog must not count as an overlay");
    }

    function test_02_dialog_open_and_close() {
        test_dialog.open();
        tryCompare(tracker, "any_open", true, 2000, "an open Dialog must hide the webview");
        test_dialog.close();
        tryCompare(tracker, "any_open", false, 2000, "closing must restore the webview");
    }

    function test_03_modal_dialog_open_and_close() {
        modal_dialog.open();
        tryCompare(tracker, "any_open", true, 2000);
        modal_dialog.close();
        tryCompare(tracker, "any_open", false, 2000,
                   "a modal popup's dimmer must not be left counted after close");
    }

    function test_04_menu() {
        test_menu.open();
        tryCompare(tracker, "any_open", true, 2000, "an open Menu must hide the webview");
        test_menu.close();
        tryCompare(tracker, "any_open", false, 2000);
    }

    // Nothing about the tracker is per-dialog, so a popup that did not exist at
    // startup must be covered too.
    function test_05_dynamically_created_dialog() {
        let dyn = dynamic_dialog_component.createObject(test_case) as Dialog;
        verify(dyn !== null, "dynamic dialog created");

        dyn.open();
        tryCompare(tracker, "any_open", true, 2000,
                   "a dialog created at runtime must flip any_open with no registration");
        dyn.close();
        tryCompare(tracker, "any_open", false, 2000);
        dyn.destroy();
    }

    function test_06_desktop_short_circuits() {
        tracker.is_mobile = false;
        test_dialog.open();
        tryCompare(test_dialog, "visible", true, 2000);
        compare(tracker.any_open, false,
                "on desktop the tracker must report false even with a dialog open");
        test_dialog.close();
        tracker.is_mobile = true;
        tryCompare(tracker, "any_open", false, 2000);
    }

    // A ToolTip is a Popup and enters the overlay like any other, but must
    // never hide the reader — on ChromeOS the pointer is real and the reader is
    // most of a large window, so a blink here is a blocking defect.
    //
    // The tracker excludes the shared ToolTip BY IDENTITY. The reason this test
    // samples the whole close transition rather than one frame: `visible` goes
    // false when the close starts, while the popupItem stays parented until the
    // end of the exit transition, so an arithmetic
    // `length - (ToolTip.toolTip.visible ? 1 : 0)` discount reads "open" for
    // exactly that interval. Do not weaken this to a single sample — it is what
    // makes a future Qt change fail in CI instead of on a user's Chromebook.
    function test_07_tooltip_alone_never_opens() {
        let shared = tracker.ToolTip.toolTip;
        verify(shared !== null && shared !== undefined, "ToolTip.toolTip must resolve");

        tooltip_host.ToolTip.show("a tooltip", 5000);
        tryCompare(test_case, "overlay_child_count", 1, 2000,
                   "the ToolTip must actually be in the overlay — otherwise this test proves nothing");
        compare(tracker.any_open, false, "a visible ToolTip alone must not hide the webview");

        shared.hide();
        compare(shared.visible, false, "tooltip visible goes false as the close starts");

        // Sample continuously until the popupItem actually leaves the overlay.
        let left_overlay = false;
        for (let i = 0; i < 400; i++) {
            compare(tracker.any_open, false,
                    "any_open must stay false across the tooltip's whole close transition");
            if (test_case.overlay_child_count === 0) {
                left_overlay = true;
                break;
            }
            wait(5);
        }
        verify(left_overlay, "the tooltip must eventually leave the overlay");
    }

    // The tooltip filter must not swallow a real popup that happens to be open
    // at the same time.
    function test_08_tooltip_plus_dialog_still_opens() {
        tooltip_host.ToolTip.show("a tooltip", 5000);
        tryCompare(test_case, "overlay_child_count", 1, 2000);
        compare(tracker.any_open, false);

        test_dialog.open();
        tryCompare(tracker, "any_open", true, 2000,
                   "a Dialog open alongside a ToolTip must still hide the webview");

        test_dialog.close();
        tracker.ToolTip.toolTip.hide();
        tryCompare(tracker, "any_open", false, 2000);
    }
}
