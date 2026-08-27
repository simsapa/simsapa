import QtQuick

// Translates between the canonical key-sequence form stored in settings and
// the labels shown to the user.
//
// On macOS Qt maps the Command key to Qt.ControlModifier (canonical "Ctrl")
// and the physical Control key to Qt.MetaModifier (canonical "Meta"). We keep
// the canonical form for storage and QKeySequence matching, but swap the
// tokens for display so users see "Command" / "Ctrl" as they expect.
QtObject {
    readonly property bool is_macos: Qt.platform.os === "osx"

    function canonical_to_display(seq: string): string {
        if (!is_macos || !seq) return seq;
        return seq.split("+").map(p => {
            let t = p.trim();
            if (t === "Ctrl") return "Command";
            if (t === "Meta") return "Ctrl";
            return p;
        }).join("+");
    }

    function display_to_canonical(seq: string): string {
        if (!is_macos || !seq) return seq;
        return seq.split("+").map(p => {
            let t = p.trim();
            if (t === "Command" || t === "Cmd") return "Ctrl";
            if (t === "Ctrl") return "Meta";
            return p;
        }).join("+");
    }
}
