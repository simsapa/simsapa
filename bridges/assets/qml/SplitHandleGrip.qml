import QtQuick

/*
 * A half-circle grip with a small triangle, drawn on a SplitView handle
 * so that users notice the separator can be dragged.
 *
 * Its flat side sits on the separator line and the round side faces into the
 * panel: to the right of a Qt.Horizontal split's handle, below a Qt.Vertical
 * split's handle. Place it with x/y next to the handle, not over it.
 *
 * The grip itself takes no input: the SplitView starts a drag only on a point
 * that the handle's containmentMask contains, so the handle's mask must also
 * call contains_handle_point() for the grip to be draggable.
 */
Item {
    id: root

    // The SplitView's orientation. Qt.Horizontal draws a half-circle facing
    // right with a triangle pointing right, Qt.Vertical a half-circle facing
    // down with a triangle pointing down.
    property int orientation: Qt.Horizontal
    property real grip_radius: 16
    property bool active: false
    property color fill_color: "#a0a0a0"
    property color active_color: "#707070"
    property color arrow_color: "#ffffff"

    readonly property bool is_horizontal: root.orientation === Qt.Horizontal
    readonly property color current_fill: (root.active || grip_hover.hovered) ? root.active_color : root.fill_color

    width: root.is_horizontal ? root.grip_radius : root.grip_radius * 2
    height: root.is_horizontal ? root.grip_radius * 2 : root.grip_radius

    // True if the point, given in handle_item's coordinates, falls inside the
    // half-circle.
    function contains_handle_point(handle_item: Item, point: point): bool {
        if (!root.visible) {
            return false;
        }
        let p = root.mapFromItem(handle_item, point.x, point.y);
        if (p.x < 0 || p.y < 0 || p.x > root.width || p.y > root.height) {
            return false;
        }
        // The circle's centre is the middle of the flat side.
        let dx = root.is_horizontal ? p.x : p.x - root.grip_radius;
        let dy = root.is_horizontal ? p.y - root.grip_radius : p.y;
        return (dx * dx + dy * dy) <= (root.grip_radius * root.grip_radius);
    }

    HoverHandler {
        id: grip_hover
    }

    Canvas {
        id: arrow_canvas
        anchors.fill: parent
        antialiasing: true

        onPaint: {
            let ctx = getContext("2d");
            ctx.reset();
            let r = root.grip_radius;

            // The half-circle is drawn here, not with a Rectangle's corner
            // radii: Qt clamps each corner radius to half the shorter side,
            // which turns a half-circle into a rounded rectangle.
            // Its centre is the middle of the flat side.
            ctx.fillStyle = String(root.current_fill);
            ctx.beginPath();
            if (root.is_horizontal) {
                ctx.moveTo(0, 0);
                ctx.arc(0, r, r, -Math.PI / 2, Math.PI / 2, false);
            } else {
                ctx.moveTo(0, 0);
                ctx.arc(r, 0, r, Math.PI, 0, true);
            }
            ctx.closePath();
            ctx.fill();

            // One small triangle on the half-circle's axis of symmetry,
            // pointing away from the line: right, or down.
            ctx.fillStyle = String(root.arrow_color);
            let ux = root.is_horizontal ? 1 : 0;
            let uy = root.is_horizontal ? 0 : 1;
            let base_off = r * 0.25;   // flat side to the triangle's base
            let depth = r * 0.4;       // base to tip
            let half = r * 0.35;       // half the base's width
            let bx = root.is_horizontal ? base_off : r;
            let by = root.is_horizontal ? r : base_off;

            ctx.beginPath();
            ctx.moveTo(bx + ux * depth, by + uy * depth);
            // The base runs perpendicular to (ux, uy).
            ctx.lineTo(bx + uy * half, by + ux * half);
            ctx.lineTo(bx - uy * half, by - ux * half);
            ctx.closePath();
            ctx.fill();
        }
    }

    onArrow_colorChanged: arrow_canvas.requestPaint()
    onCurrent_fillChanged: arrow_canvas.requestPaint()
    onOrientationChanged: arrow_canvas.requestPaint()
    onGrip_radiusChanged: arrow_canvas.requestPaint()
}
