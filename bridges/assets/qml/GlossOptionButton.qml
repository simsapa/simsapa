import QtQuick
import QtQuick.Controls

// A flat icon button of the Gloss tab: the option buttons under the main
// input (an icon and a short abbreviation, `checked` bound to the option's
// state) and the rows' dictionary lookup buttons (icon only). Flat, so it
// blends into the row it is placed in; Fusion tints the icon with the
// palette's button text colour, which follows the light / dark theme.
//
// Not `checkable`, so a click never replaces a `checked` binding; the click
// handler sets the option. In the option row each one sets `implicitHeight`
// to the Export button's: a Flow aligns its items at the top, and Fusion's
// ToolButton is taller than its Button.
ToolButton {
    property string tip: ""
    display: AbstractButton.TextBesideIcon
    font.pointSize: 10
    topPadding: 3
    bottomPadding: 3
    icon.width: 16
    icon.height: 16
    ToolTip.visible: hovered && tip.length > 0
    ToolTip.delay: 500
    ToolTip.text: tip
}
