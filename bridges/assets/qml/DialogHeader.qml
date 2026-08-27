import QtQuick
import QtQuick.Controls

// A `header:` for a Dialog, visually identical to Fusion's own but without the
// binding loop it causes.
//
// Fusion's Dialog computes
//   implicitHeight: … + (implicitHeaderHeight > 0 ? implicitHeaderHeight + spacing : 0) + …
// (Fusion/Dialog.qml:17-20), and its default header is a Label whose
// implicitHeight is resolved through that same layout pass. On its own that is
// stable — but combined with content whose height varies with its width (any
// `wrapMode: Text.WordWrap` label) it oscillates, and Qt reports
//
//   QML Dialog: Binding loop detected for property "implicitHeight"
//
// on every window resize. Both halves are required: measured across a variant
// matrix, removing `wrapMode` fixes it, `header: null` fixes it, and stating the
// header's implicitHeight outright fixes it — while the content sizing
// (availableWidth, an explicit contentItem, an explicit content height),
// `standardButtons`, `anchors.centerIn`, `parent: Overlay.overlay` and the
// dialog's width clamp all make no difference.
//
// The fix is the `implicitHeight` line below. It equals what the Label's own
// implicit height already is, so it changes no geometry — it only states the
// value outright instead of letting it be resolved through the layout, which is
// what breaks the feedback path. Do not "simplify" it away, and do not replace
// it with a constant: this form carries no magic number and no DPI or font
// assumption.
//
// The root is an `Item` wrapping the Label, not a Label, because
// `implicitHeight` is READ-ONLY on Label (it derives from the text) — assigning
// it there is a load error, not an override, and the component silently fails
// to load.
//
// Usage — pass the dialog's own title, since a header's parent is the
// popupItem, not the Dialog:
//
//     Dialog {
//         id: my_dialog
//         title: "Something"
//         header: DialogHeader { text: my_dialog.title }
//     }
Item {
    id: root

    property alias text: label.text
    readonly property int label_padding: 6

    visible: label.text.length > 0
    implicitWidth: label.implicitWidth
    implicitHeight: label.contentHeight + 2 * root.label_padding

    // Fusion's own header background, reproduced so the dialogs look unchanged.
    Rectangle {
        x: 1
        y: 1
        width: root.width - 2
        height: root.height - 1
        color: label.palette.window
        radius: 2
    }

    Label {
        id: label
        anchors.fill: parent
        padding: root.label_padding
        elide: Label.ElideRight
        font.bold: true
        verticalAlignment: Text.AlignVCenter
    }
}
