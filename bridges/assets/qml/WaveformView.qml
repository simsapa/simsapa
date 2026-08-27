pragma ComponentBehavior: Bound

import QtQuick

Item {
    id: root

    // Input properties
    property var waveform_data: []       // Array of floats 0.0-1.0
    property int duration_ms: 0          // Total duration in ms
    property int playback_position_ms: 0 // Current playback position in ms
    property bool is_playing: false
    property var markers: []             // Array of marker objects from markers_json

    // Visual properties
    property color bar_color_played: palette.highlight
    property color bar_color_unplayed: palette.mid
    property color bar_color_range: Qt.rgba(palette.highlight.r, palette.highlight.g, palette.highlight.b, 0.3)
    property color position_marker_color: "red"
    property color cursor_color: palette.text
    property real bar_spacing: waveform_data.length > 0 && root.width > 0
        ? Math.min(1, root.width / waveform_data.length * 0.3)
        : 1
    property int min_bar_height: 2

    // Range creation state (passed from parent)
    property bool range_create_active: false   // True when in range creation mode
    property int range_create_pending_ms: -1   // Start position during range creation

    // Drag state for range selection
    property bool is_dragging: false
    property real drag_start_x: 0
    property real drag_current_x: 0

    // Signals
    signal seek_requested(int position_ms)
    signal range_selected(int start_ms, int end_ms)

    implicitHeight: 60

    // Convert x position to time in ms
    function x_to_ms(x: real): int {
        if (root.width <= 0 || root.duration_ms <= 0) return 0;
        return Math.round(Math.max(0, Math.min(1, x / root.width)) * root.duration_ms);
    }

    // Convert time in ms to x position
    function ms_to_x(ms: int): real {
        if (root.duration_ms <= 0) return 0;
        return (ms / root.duration_ms) * root.width;
    }

    // Parse position markers from the markers array
    function get_position_markers(): var {
        let result = [];
        for (let i = 0; i < root.markers.length; i++) {
            if (root.markers[i].type === "position") {
                result.push(root.markers[i]);
            }
        }
        return result;
    }

    // Parse range markers from the markers array
    function get_range_markers(): var {
        let result = [];
        for (let i = 0; i < root.markers.length; i++) {
            if (root.markers[i].type === "range") {
                result.push(root.markers[i]);
            }
        }
        return result;
    }

    clip: true

    // Background
    Rectangle {
        anchors.fill: parent
        color: "transparent"
    }

    // Range marker backgrounds (rendered behind waveform bars)
    Repeater {
        id: range_marker_repeater
        model: root.get_range_markers()

        Rectangle {
            required property var modelData

            x: root.ms_to_x(modelData.start_ms)
            y: 0
            width: root.ms_to_x(modelData.end_ms) - x
            height: root.height
            color: root.bar_color_range
            radius: 2
        }
    }

    // Waveform bars
    Row {
        id: bars_row
        anchors.fill: parent
        spacing: root.bar_spacing

        Repeater {
            id: bar_repeater
            model: root.waveform_data.length

            Rectangle {
                required property int index

                property real amplitude: index < root.waveform_data.length ? root.waveform_data[index] : 0
                property real bar_progress: root.waveform_data.length > 0
                    ? (index + 0.5) / root.waveform_data.length
                    : 0
                property real playback_progress: root.duration_ms > 0
                    ? root.playback_position_ms / root.duration_ms
                    : 0
                property bool is_played: bar_progress <= playback_progress

                width: root.waveform_data.length > 0
                    ? (bars_row.width - (root.waveform_data.length - 1) * root.bar_spacing) / root.waveform_data.length
                    : 1
                height: Math.max(root.min_bar_height, amplitude * bars_row.height)
                y: bars_row.height - height
                color: is_played ? root.bar_color_played : root.bar_color_unplayed
                radius: Math.min(1, width / 2)
            }
        }
    }

    // Position markers (thin vertical lines)
    Repeater {
        id: position_marker_repeater
        model: root.get_position_markers()

        Rectangle {
            required property var modelData

            x: root.ms_to_x(modelData.position_ms) - 1
            y: 0
            width: 2
            height: root.height
            color: root.position_marker_color
        }
    }

    // Pending range start marker (shown during range creation)
    Rectangle {
        visible: root.range_create_pending_ms >= 0
        x: root.ms_to_x(root.range_create_pending_ms) - 1
        y: 0
        width: 2
        height: root.height
        color: palette.highlight

        // Triangular indicator at the top
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.top: parent.top
            width: 8
            height: 8
            color: palette.highlight
            rotation: 45
        }
    }

    // Playback cursor
    Rectangle {
        id: playback_cursor
        x: root.ms_to_x(root.playback_position_ms) - 1
        y: 0
        width: 2
        height: root.height
        color: root.cursor_color
        visible: root.duration_ms > 0
    }

    // Drag preview rectangle for range selection
    Rectangle {
        id: drag_preview
        visible: root.is_dragging
        y: 0
        height: root.height
        x: Math.min(root.drag_start_x, root.drag_current_x)
        width: Math.abs(root.drag_current_x - root.drag_start_x)
        color: Qt.rgba(palette.highlight.r, palette.highlight.g, palette.highlight.b, 0.2)
        border.color: palette.highlight
        border.width: 1
        radius: 2
    }

    // Mouse interaction area
    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor

        onPressed: function(mouse) {
            root.is_dragging = true;
            root.drag_start_x = mouse.x;
            root.drag_current_x = mouse.x;
        }

        onPositionChanged: function(mouse) {
            if (root.is_dragging) {
                root.drag_current_x = Math.max(0, Math.min(root.width, mouse.x));
            }
        }

        onReleased: function(mouse) {
            root.is_dragging = false;
            let end_x = Math.max(0, Math.min(root.width, mouse.x));
            let drag_distance = Math.abs(end_x - root.drag_start_x);

            if (root.range_create_active || drag_distance < 5) {
                // During range creation, always treat as a position click.
                // Otherwise, short click: seek to position.
                root.seek_requested(root.x_to_ms(end_x));
            } else {
                // Drag: create range
                let start_ms = root.x_to_ms(Math.min(root.drag_start_x, end_x));
                let end_ms = root.x_to_ms(Math.max(root.drag_start_x, end_x));
                root.range_selected(start_ms, end_ms);
            }
        }
    }
}
