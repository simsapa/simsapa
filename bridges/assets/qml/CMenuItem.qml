// Copyright (C) 2017 The Qt Company Ltd.
// SPDX-License-Identifier: LicenseRef-Qt-Commercial OR LGPL-3.0-only OR GPL-2.0-only OR GPL-3.0-only

/* Based on Fusion style.
 * ~/Qt/6.9.3/gcc_64/qml/QtQuick/Controls/Fusion/MenuItem.qml
 * Added the feature to show the text of shortcut sequences.
 */

import QtQuick
import QtQuick.Templates as T
import QtQuick.Controls.impl
import QtQuick.Controls.Fusion
import QtQuick.Controls.Fusion.impl

T.MenuItem {
    id: control

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !control.is_mobile

    signal action_triggered()

    function connect_action() {
        control.action.triggered.connect(control.action_triggered);
    }

    property KeySequenceDisplay key_seq_display: KeySequenceDisplay {}

    function action_seq_to_str(action) {
        if (!action) return "";
        if (!action.shortcut) return "";
        if (!action.shortcut.sequences) return "";

        let seq = action.shortcut.sequences;
        let a = [];
        for (var i = 0; i < seq.length; i++) {
            a.push(control.key_seq_display.canonical_to_display(seq[i].toString()));
        }
        return a.join(", ");
    }

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             implicitIndicatorHeight + topPadding + bottomPadding)

    padding: 6
    spacing: 6

    icon.width: 16
    icon.height: 16

    contentItem: Item {
        IconLabel {
            readonly property real arrowPadding: control.subMenu && control.arrow ? control.arrow.width + control.spacing : 0
            readonly property real indicatorPadding: control.checkable && control.indicator ? control.indicator.width + control.spacing : 0
            leftPadding: !control.mirrored ? indicatorPadding : arrowPadding
            rightPadding: control.mirrored ? indicatorPadding : arrowPadding

            spacing: control.spacing
            mirrored: control.mirrored
            display: control.display
            alignment: Qt.AlignLeft

            icon: control.icon
            text: control.text
            font: control.font
            color: control.down || control.highlighted ? Fusion.highlightedText(control.palette) : control.palette.text
        }

        Text {
            visible: control.is_desktop
            text: control.action_seq_to_str(control.action)
            anchors.right: parent.right
            color: control.down || control.highlighted ? Fusion.highlightedText(control.palette) : control.palette.text
        }
    }

    arrow: ColorImage {
        x: control.mirrored ? control.padding : control.width - width - control.padding
        y: control.topPadding + (control.availableHeight - height) / 2
        width: 20

        visible: control.subMenu
        rotation: control.mirrored ? 90 : -90
        color: control.down || control.hovered || control.highlighted ? Fusion.highlightedText(control.palette) : control.palette.text
        source: "qrc:/qt-project.org/imports/QtQuick/Controls/Fusion/images/arrow.png"
        fillMode: Image.Pad
    }

    indicator: CheckIndicator {
        x: control.mirrored ? control.width - width - control.rightPadding : control.leftPadding
        y: control.topPadding + (control.availableHeight - height) / 2

        control: control
        visible: control.checkable
    }

    background: Rectangle {
        implicitWidth: 200
        implicitHeight: 20

        color: Fusion.highlight(control.palette)
        visible: control.down || control.highlighted
    }
}
