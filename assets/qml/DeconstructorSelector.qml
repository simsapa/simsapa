pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

// Shared break-down selector: a ComboBox of deconstructor break-downs
// (`words_joined` strings) plus a checkable lock ToolButton. When locked, the
// embedding view filters its displayed results to the selected break-down's
// components (plus direct matches). See PRD FR-B1 and
// docs/gloss-ai-word-selection.md (lock semantics).
//
// The ComboBox emits changes only via `onActivated` (never
// `onCurrentIndexChanged`), mirroring the sense-selection rule, so programmatic
// index changes (e.g. restoring a saved selection) do not fire `activated`.
RowLayout {
    id: root

    // List of break-down display strings, e.g. ["sādhu + iti", "sādhū + iti"].
    property var model: []
    // Currently selected break-down index.
    property int current_index: 0
    // Whether the selection is locked (filters the result list).
    property bool locked: false

    // Control sizing: the ComboBox height and the (square) lock button side.
    // Defaults to the ComboBox's natural implicit height so embedders that
    // don't set it (e.g. GlossTab) keep today's appearance; WordSummary and
    // FulltextResults set it to match their sibling buttons' height.
    property int control_size: breakdown_combo.implicitHeight

    // Emitted when the user picks a different break-down from the ComboBox.
    signal activated(int index)
    // Emitted when the user toggles the lock button.
    signal lock_toggled(bool locked)

    spacing: 4

    ComboBox {
        id: breakdown_combo
        Layout.fillWidth: true
        Layout.preferredHeight: root.control_size
        model: root.model
        currentIndex: root.current_index

        // Keep the visible selection in sync when the property is set
        // programmatically (restore / break-down switch) without emitting
        // `activated`.
        Binding {
            target: breakdown_combo
            property: "currentIndex"
            value: root.current_index
        }

        onActivated: (index) => {
            root.current_index = index;
            root.activated(index);
        }
    }

    ToolButton {
        id: lock_btn
        checkable: true
        checked: root.locked
        icon.source: root.locked ? "icons/32x32/system-uicons--lock.png"
                                 : "icons/32x32/system-uicons--lock-open.png"
        Layout.preferredHeight: root.control_size
        Layout.preferredWidth: root.control_size
        ToolTip.visible: hovered
        ToolTip.text: root.locked ? "Unlock: show all break-downs" : "Lock: show only this break-down"

        onClicked: {
            root.locked = lock_btn.checked;
            root.lock_toggled(lock_btn.checked);
        }
    }
}
