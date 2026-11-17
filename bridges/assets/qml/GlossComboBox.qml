// Derived from the Fusion style's ComboBox.qml (Qt 6.9.3).
// Copyright (C) 2017 The Qt Company Ltd.
// SPDX-License-Identifier: LicenseRef-Qt-Commercial OR LGPL-3.0-only OR GPL-2.0-only OR GPL-3.0-only

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Window
import QtQuick.Templates as T
import QtQuick.Controls
import QtQuick.Controls.impl

// A ComboBox for the Gloss tab's rows. It blends into the row: its background
// is `bg_color` (the row's colour), with only a bottom border and the
// drop-down arrow drawn in `text_color`, so the text reads like the row's
// other cells. The popup list is the Fusion one.
//
// Not editable: the Gloss tab only selects. React to a choice in
// `onActivated`, never `onCurrentIndexChanged`.
T.ComboBox {
    id: control

    // The background of the row the ComboBox is placed in.
    property color bg_color: "transparent"
    // The row's text colour: the text, the arrow and the bottom border.
    property color text_color: control.palette.buttonText

    // The hover and pressed tint, and the resting bottom border, are
    // `text_color` at these opacities, so they work on a light or a dark row.
    readonly property real hover_tint_opacity: 0.06
    readonly property real down_tint_opacity: 0.12
    readonly property real border_opacity: 0.35

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             implicitIndicatorHeight + topPadding + bottomPadding)

    leftPadding: padding + (!control.mirrored || !indicator || !indicator.visible ? 0 : indicator.width + spacing)
    rightPadding: padding + (control.mirrored || !indicator || !indicator.visible ? 0 : indicator.width + spacing)

    delegate: MenuItem {
        required property var model
        required property int index

        width: ListView.view.width
        text: model[control.textRole]
        font.weight: control.currentIndex === index ? Font.DemiBold : Font.Normal
        highlighted: control.highlightedIndex === index
        hoverEnabled: control.hoverEnabled
    }

    indicator: ColorImage {
        x: control.mirrored ? control.padding : control.width - width - control.padding
        y: control.topPadding + (control.availableHeight - height) / 2
        color: control.text_color
        source: "qrc:/qt-project.org/imports/QtQuick/Controls/Fusion/images/arrow.png"
        width: 20
        fillMode: Image.Pad
        opacity: control.enabled ? 1.0 : 0.5
    }

    contentItem: Text {
        leftPadding: 4 - control.padding
        rightPadding: 4 - control.padding
        topPadding: 4
        bottomPadding: 4

        text: control.displayText
        font: control.font
        color: control.text_color
        opacity: control.enabled ? 1.0 : 0.5
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }

    background: Rectangle {
        implicitWidth: 120
        implicitHeight: 24
        color: control.bg_color

        // Hover / pressed feedback.
        Rectangle {
            anchors.fill: parent
            color: control.text_color
            opacity: control.down || control.popup.visible
                ? control.down_tint_opacity
                : (control.hovered && control.enabled ? control.hover_tint_opacity : 0)
        }

        // The bottom border: thicker and in the highlight colour with focus.
        Rectangle {
            readonly property bool is_focused: control.visualFocus || control.popup.visible
            x: 0
            y: parent.height - height
            width: parent.width
            height: is_focused ? 2 : 1
            color: is_focused ? control.palette.highlight : control.text_color
            opacity: is_focused ? 1.0 : control.border_opacity
        }
    }

    popup: T.Popup {
        width: control.width
        height: Math.min(contentItem.implicitHeight + 2, control.Window.height - topMargin - bottomMargin)
        topMargin: 6
        bottomMargin: 6
        padding: 1
        palette: control.palette

        contentItem: ListView {
            clip: true
            implicitHeight: contentHeight
            model: control.delegateModel
            currentIndex: control.highlightedIndex
            highlightRangeMode: ListView.ApplyRange
            highlightMoveDuration: 0

            T.ScrollBar.vertical: ScrollBar { }
        }

        background: Rectangle {
            color: control.popup.palette.window
            border.color: control.palette.mid

            Rectangle {
                z: -1
                x: 1; y: 1
                width: parent.width
                height: parent.height
                color: control.palette.shadow
                opacity: 0.2
            }
        }
    }
}
