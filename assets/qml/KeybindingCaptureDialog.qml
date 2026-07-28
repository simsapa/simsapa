pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

ApplicationWindow {
    id: root
    title: "Capture Keyboard Shortcut"
    width: is_mobile ? Screen.desktopAvailableWidth : 450
    height: is_mobile ? Screen.desktopAvailableHeight : 400
    visible: false
    color: palette.window
    flags: Qt.Dialog
    modality: Qt.ApplicationModal

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile
    readonly property bool is_macos: Qt.platform.os === "osx"

    KeySequenceDisplay { id: key_seq_display }

    readonly property string display_shortcut: key_seq_display.canonical_to_display(root.captured_shortcut)

    readonly property int pointSize: is_mobile ? 14 : 12
    required property int extra_top_margin

    // Properties for the dialog
    property string action_name: ""
    property string current_shortcut: ""
    property bool is_new_shortcut: true

    // When true, the dialog captures double-tap chord sequences of the form
    // `Modifier+Key+Key` (e.g. `Ctrl+C+C`) in addition to ordinary single
    // chords. Used by global hotkeys per the Goldendict-ng convention.
    property bool allow_double_tap: false

    // Double-tap timeout in milliseconds; matches Goldendict's behaviour.
    readonly property int double_tap_timeout_ms: 500

    // Internal state
    property string captured_shortcut: ""
    property bool is_valid_shortcut: false

    // Double-tap capture state.
    property bool waiting_for_second_key: false
    property string first_chord_sequence: ""
    property int first_chord_modifiers: 0

    // Signals
    signal shortcutAccepted(string shortcut)
    signal shortcutRemoved()

    property bool is_dark: theme_helper.is_dark

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    Component.onCompleted: {
        theme_helper.apply();
    }

    onVisibleChanged: {
        if (visible) {
            // Reset state when dialog becomes visible
            root.captured_shortcut = root.current_shortcut;
            root.is_valid_shortcut = root.current_shortcut !== ""
                                      && root.is_sequence_valid(root.current_shortcut);
            root.waiting_for_second_key = false;
            root.first_chord_sequence = "";
            root.first_chord_modifiers = 0;
            second_key_timer.stop();
            key_capture_area.forceActiveFocus();
        }
    }

    // 500 ms window in which a second key press is treated as the second tap
    // of a double-tap chord (e.g. the second `C` in `Ctrl+C+C`).
    Timer {
        id: second_key_timer
        interval: root.double_tap_timeout_ms
        repeat: false
        onTriggered: {
            // Timeout: keep the single chord as the captured result.
            root.waiting_for_second_key = false;
        }
    }

    // Validate a key-sequence string. Accepts:
    //   - single chord:    `Modifier(+Modifier)*+Key`     e.g. Ctrl+Shift+D
    //   - double-tap:      `Modifier(+Modifier)*+Key+Key` e.g. Ctrl+C+C
    //   - plain function key: `F1`..`F12`
    function is_sequence_valid(seq: string): bool {
        let s = seq.trim();
        if (s === "") {
            return false;
        }
        let parts = s.split("+").map(p => p.trim()).filter(p => p !== "");
        if (parts.length === 0) {
            return false;
        }
        const modifiers = ["Ctrl", "Alt", "Shift", "Meta"];
        // Trailing parts (key, optional second key) must not be modifiers.
        let last = parts[parts.length - 1];
        if (modifiers.indexOf(last) !== -1) {
            return false;
        }
        // Everything except the final 1 or 2 parts must be modifiers.
        let key_count = root.allow_double_tap ? 2 : 1;
        let mod_end = parts.length - 1;
        if (root.allow_double_tap
            && parts.length >= 2
            && modifiers.indexOf(parts[parts.length - 2]) === -1) {
            mod_end = parts.length - 2;
        }
        for (let i = 0; i < mod_end; i++) {
            if (modifiers.indexOf(parts[i]) === -1) {
                return false;
            }
        }
        return true;
    }

    // Helper function to check if a key is a modifier key
    function is_modifier_key(key: int): bool {
        return key === Qt.Key_Control ||
               key === Qt.Key_Shift ||
               key === Qt.Key_Alt ||
               key === Qt.Key_Meta;
    }

    // Helper function to build shortcut string from key event
    function build_key_sequence(event): string {
        let parts = [];

        let has_shift = event.modifiers & Qt.ShiftModifier;

        // Add modifiers in standard order
        if (event.modifiers & Qt.ControlModifier) {
            parts.push("Ctrl");
        }
        if (event.modifiers & Qt.AltModifier) {
            parts.push("Alt");
        }
        if (has_shift) {
            parts.push("Shift");
        }
        if (event.modifiers & Qt.MetaModifier) {
            parts.push("Meta");
        }

        // Get the key name, passing shift state to handle shifted characters
        let key_name = get_key_name(event.key, event.text, has_shift);
        if (key_name !== "") {
            parts.push(key_name);
        }

        return parts.join("+");
    }

    // Helper function to get Qt key name from key code
    function get_key_name(key: int, text: string, has_shift: bool): string {
        // Map shifted characters back to base keys (US keyboard layout)
        // When Shift is pressed, Qt may report the shifted character's key code
        const shifted_to_base = {
            [Qt.Key_Plus]: "=",        // Shift+= gives +
            [Qt.Key_Exclam]: "1",      // Shift+1 gives !
            [Qt.Key_At]: "2",          // Shift+2 gives @
            [Qt.Key_NumberSign]: "3",  // Shift+3 gives #
            [Qt.Key_Dollar]: "4",      // Shift+4 gives $
            [Qt.Key_Percent]: "5",     // Shift+5 gives %
            [Qt.Key_AsciiCircum]: "6", // Shift+6 gives ^
            [Qt.Key_Ampersand]: "7",   // Shift+7 gives &
            [Qt.Key_Asterisk]: "8",    // Shift+8 gives *
            [Qt.Key_ParenLeft]: "9",   // Shift+9 gives (
            [Qt.Key_ParenRight]: "0",  // Shift+0 gives )
            [Qt.Key_Underscore]: "-",  // Shift+- gives _
            [Qt.Key_BraceLeft]: "[",   // Shift+[ gives {
            [Qt.Key_BraceRight]: "]",  // Shift+] gives }
            [Qt.Key_Bar]: "\\",        // Shift+\ gives |
            [Qt.Key_Colon]: ";",       // Shift+; gives :
            [Qt.Key_QuoteDbl]: "'",    // Shift+' gives "
            [Qt.Key_Less]: ",",        // Shift+, gives <
            [Qt.Key_Greater]: ".",     // Shift+. gives >
            [Qt.Key_Question]: "/",    // Shift+/ gives ?
            [Qt.Key_AsciiTilde]: "`",  // Shift+` gives ~
        };

        // If Shift is pressed and we have a shifted character, map it back to base
        if (has_shift && key in shifted_to_base) {
            return shifted_to_base[key];
        }

        // Map common keys to their Qt names
        const key_map = {
            [Qt.Key_Escape]: "Escape",
            [Qt.Key_Tab]: "Tab",
            [Qt.Key_Backtab]: "Backtab",
            [Qt.Key_Backspace]: "Backspace",
            [Qt.Key_Return]: "Return",
            [Qt.Key_Enter]: "Enter",
            [Qt.Key_Insert]: "Insert",
            [Qt.Key_Delete]: "Delete",
            [Qt.Key_Pause]: "Pause",
            [Qt.Key_Print]: "Print",
            [Qt.Key_Home]: "Home",
            [Qt.Key_End]: "End",
            [Qt.Key_Left]: "Left",
            [Qt.Key_Up]: "Up",
            [Qt.Key_Right]: "Right",
            [Qt.Key_Down]: "Down",
            [Qt.Key_PageUp]: "PageUp",
            [Qt.Key_PageDown]: "PageDown",
            [Qt.Key_Space]: "Space",
            [Qt.Key_F1]: "F1",
            [Qt.Key_F2]: "F2",
            [Qt.Key_F3]: "F3",
            [Qt.Key_F4]: "F4",
            [Qt.Key_F5]: "F5",
            [Qt.Key_F6]: "F6",
            [Qt.Key_F7]: "F7",
            [Qt.Key_F8]: "F8",
            [Qt.Key_F9]: "F9",
            [Qt.Key_F10]: "F10",
            [Qt.Key_F11]: "F11",
            [Qt.Key_F12]: "F12",
            [Qt.Key_Comma]: ",",
            [Qt.Key_Period]: ".",
            [Qt.Key_Slash]: "/",
            [Qt.Key_Backslash]: "\\",
            [Qt.Key_Semicolon]: ";",
            [Qt.Key_Apostrophe]: "'",
            [Qt.Key_BracketLeft]: "[",
            [Qt.Key_BracketRight]: "]",
            [Qt.Key_Minus]: "-",
            [Qt.Key_Equal]: "=",
            [Qt.Key_QuoteLeft]: "`",
            // Number keys
            [Qt.Key_0]: "0",
            [Qt.Key_1]: "1",
            [Qt.Key_2]: "2",
            [Qt.Key_3]: "3",
            [Qt.Key_4]: "4",
            [Qt.Key_5]: "5",
            [Qt.Key_6]: "6",
            [Qt.Key_7]: "7",
            [Qt.Key_8]: "8",
            [Qt.Key_9]: "9",
            // Letter keys (Qt.Key_A = 0x41 = 65, etc.)
            [Qt.Key_A]: "A",
            [Qt.Key_B]: "B",
            [Qt.Key_C]: "C",
            [Qt.Key_D]: "D",
            [Qt.Key_E]: "E",
            [Qt.Key_F]: "F",
            [Qt.Key_G]: "G",
            [Qt.Key_H]: "H",
            [Qt.Key_I]: "I",
            [Qt.Key_J]: "J",
            [Qt.Key_K]: "K",
            [Qt.Key_L]: "L",
            [Qt.Key_M]: "M",
            [Qt.Key_N]: "N",
            [Qt.Key_O]: "O",
            [Qt.Key_P]: "P",
            [Qt.Key_Q]: "Q",
            [Qt.Key_R]: "R",
            [Qt.Key_S]: "S",
            [Qt.Key_T]: "T",
            [Qt.Key_U]: "U",
            [Qt.Key_V]: "V",
            [Qt.Key_W]: "W",
            [Qt.Key_X]: "X",
            [Qt.Key_Y]: "Y",
            [Qt.Key_Z]: "Z",
        };

        if (key in key_map) {
            return key_map[key];
        }

        // Fallback: For other keys, try using the text if it's a printable character
        if (text && text.length === 1) {
            let char_code = text.charCodeAt(0);
            // Printable ASCII characters
            if (char_code >= 32 && char_code <= 126) {
                return text.toUpperCase();
            }
        }

        return "";
    }

    // Build display string showing current modifiers being held
    function build_modifier_display(event): string {
        let parts = [];

        if (event.modifiers & Qt.ControlModifier) {
            parts.push("Ctrl");
        }
        if (event.modifiers & Qt.AltModifier) {
            parts.push("Alt");
        }
        if (event.modifiers & Qt.ShiftModifier) {
            parts.push("Shift");
        }
        if (event.modifiers & Qt.MetaModifier) {
            parts.push("Meta");
        }

        if (parts.length > 0) {
            return parts.join("+") + "+...";
        }
        return "";
    }

    Frame {
        anchors.fill: parent

        ColumnLayout {
            spacing: 15
            anchors.fill: parent
            anchors.topMargin: root.extra_top_margin
            anchors.margins: 15

            // Title
            Label {
                text: root.is_new_shortcut ? `Add shortcut for "${root.action_name}"` : `Edit shortcut for "${root.action_name}"`
                font.bold: true
                font.pointSize: root.pointSize + 2
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
            }

            // Instructions
            Label {
                text: {
                    if (root.allow_double_tap) {
                        if (root.waiting_for_second_key) {
                            return "Now press the second key (e.g. " + key_seq_display.canonical_to_display(root.first_chord_sequence)
                                + " then the key again)…";
                        }
                        return "Press the modifier+key combination, then press the second key:";
                    }
                    return "Press the key combination you want to use:";
                }
                font.pointSize: root.pointSize
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
            }

            // Key capture area
            Rectangle {
                id: key_capture_area
                Layout.fillWidth: true
                Layout.preferredHeight: 60
                color: root.waiting_for_second_key
                       ? (root.is_dark ? "#3a4a2a" : "#f0f8e8")
                       : (key_capture_area.activeFocus
                          ? (root.is_dark ? "#2a3a4a" : "#e8f0f8")
                          : (root.is_dark ? "#1a2a3a" : "#f0f0f0"))
                border.color: root.waiting_for_second_key
                              ? "#7aa84a"
                              : (key_capture_area.activeFocus ? palette.highlight : palette.mid)
                border.width: (key_capture_area.activeFocus || root.waiting_for_second_key) ? 2 : 1
                radius: 6

                focus: true

                Label {
                    anchors.centerIn: parent
                    text: root.captured_shortcut !== "" ? root.display_shortcut : (key_capture_area.activeFocus ? "Press a key combination..." : "Click here to capture shortcut")
                    font.pointSize: root.pointSize + 2
                    font.bold: root.is_valid_shortcut
                    color: root.is_valid_shortcut ? palette.text : palette.placeholderText
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: key_capture_area.forceActiveFocus()
                }

                Keys.onShortcutOverride: function(event) {
                    // Override all shortcuts to prevent them from being triggered
                    // This allows us to capture key combinations that are already assigned
                    event.accepted = true;
                }

                Keys.onPressed: function(event) {
                    event.accepted = true;

                    if (root.is_modifier_key(event.key)) {
                        // Only modifier pressed - show partial state.
                        // Don't reset double-tap waiting state: the user is
                        // still holding the modifier between the two taps.
                        if (!root.waiting_for_second_key) {
                            // build_modifier_display returns canonical tokens
                            // (Ctrl/Meta); display_shortcut will swap them on
                            // macOS for the user-visible label.
                            root.captured_shortcut = root.build_modifier_display(event);
                            root.is_valid_shortcut = false;
                        }
                        return;
                    }

                    if (root.allow_double_tap && root.waiting_for_second_key) {
                        // Second tap: append the bare key name (no modifiers)
                        // to the first chord, e.g. "Ctrl+C" → "Ctrl+C+C".
                        let has_shift = !!(event.modifiers & Qt.ShiftModifier);
                        let key_name = root.get_key_name(event.key, event.text, has_shift);
                        if (key_name !== "") {
                            root.captured_shortcut = root.first_chord_sequence + "+" + key_name;
                            root.is_valid_shortcut = root.is_sequence_valid(root.captured_shortcut);
                            root.waiting_for_second_key = false;
                            second_key_timer.stop();
                        }
                        return;
                    }

                    // First (or only) chord.
                    let shortcut = root.build_key_sequence(event);
                    if (shortcut === "") {
                        return;
                    }
                    root.captured_shortcut = shortcut;
                    root.is_valid_shortcut = root.is_sequence_valid(shortcut);

                    if (root.allow_double_tap) {
                        // Enter waiting state; if no second key arrives within
                        // the timeout, the single chord is kept as-is.
                        root.first_chord_sequence = shortcut;
                        root.first_chord_modifiers = event.modifiers;
                        root.waiting_for_second_key = true;
                        second_key_timer.restart();
                    }
                }

                Keys.onReleased: function(event) {
                    event.accepted = true;

                    // If we were showing a partial modifier state and all modifiers are released,
                    // keep showing the incomplete shortcut until a new key is pressed
                    if (!root.is_valid_shortcut && event.modifiers === Qt.NoModifier) {
                        // Keep the current display or clear if nothing valid
                        if (root.captured_shortcut.endsWith("...")) {
                            // Keep partial display visible
                        }
                    }
                }
            }

            // Inline error: shown when text is non-empty but parses as invalid.
            Label {
                visible: root.captured_shortcut.trim() !== "" && !root.is_valid_shortcut
                text: root.allow_double_tap
                      ? "Invalid sequence. Expected Modifier+Key or Modifier+Key+Key (e.g. " + (root.is_macos ? "Command+C+C" : "Ctrl+C+C") + ")."
                      : "Invalid sequence. Expected Modifier+Key (e.g. " + (root.is_macos ? "Command+Shift+S" : "Ctrl+Shift+S") + ")."
                font.pointSize: root.pointSize - 1
                color: "#c0392b"
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            // Current shortcut info (when editing)
            Label {
                visible: !root.is_new_shortcut && root.current_shortcut !== ""
                text: `Current: ${key_seq_display.canonical_to_display(root.current_shortcut)}`
                font.pointSize: root.pointSize - 1
                color: palette.placeholderText
                Layout.fillWidth: true
            }

            // Manual text input
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 10
                spacing: 10

                Label {
                    text: "Or enter manually:"
                    font.pointSize: root.pointSize
                }

                TextField {
                    id: manual_input
                    Layout.fillWidth: true
                    font.pointSize: root.pointSize
                    placeholderText: root.is_macos ? "e.g. Command+Shift+S" : "e.g. Ctrl+Shift+S"

                    // Update text field when captured_shortcut changes from key capture
                    Connections {
                        target: root
                        function onCaptured_shortcutChanged() {
                            if (!manual_input.activeFocus) {
                                manual_input.text = root.display_shortcut;
                            }
                        }
                    }

                    onTextEdited: {
                        // User types display form (Command/Ctrl on macOS); store
                        // canonical so QKeySequence and storage stay platform-neutral.
                        let canonical = key_seq_display.display_to_canonical(text);
                        root.captured_shortcut = canonical;
                        root.is_valid_shortcut = root.is_sequence_valid(canonical);
                    }

                    onAccepted: {
                        // Allow pressing Enter to accept
                        if (root.is_valid_shortcut) {
                            root.shortcutAccepted(root.captured_shortcut);
                            root.close();
                        }
                    }

                    Component.onCompleted: {
                        text = root.display_shortcut;
                    }
                }
            }

            Label {
                text: `For reference, see the <a href="https://doc.qt.io/qt-6/qkeysequence.html#standard-shortcuts">QKeySequence Class</a> and the values of <a href="https://doc.qt.io/qt-6/qt.html#Key-enum">Qt::Key</a>.`
                font.pointSize: root.pointSize - 2
                color: palette.text
                textFormat: Text.RichText
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
                Layout.bottomMargin: 8
                onLinkActivated: (link) => Qt.openUrlExternally(link)
                MouseArea {
                    anchors.fill: parent
                    acceptedButtons: Qt.NoButton // we don't want to eat clicks on the Text
                    cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor
                }
            }

            Item { Layout.fillHeight: true }

            // Buttons
            RowLayout {
                spacing: 10
                Layout.fillWidth: true
                Layout.bottomMargin: root.is_mobile ? 60 : 10

                Button {
                    text: "Clear"
                    font.pointSize: root.pointSize
                    onClicked: {
                        root.captured_shortcut = "";
                        root.is_valid_shortcut = false;
                        key_capture_area.forceActiveFocus();
                    }
                }

                Item { Layout.fillWidth: true }

                Button {
                    text: "Remove"
                    visible: !root.is_new_shortcut
                    font.pointSize: root.pointSize
                    onClicked: {
                        root.shortcutRemoved();
                        root.close();
                    }
                }

                Button {
                    text: "Cancel"
                    font.pointSize: root.pointSize
                    onClicked: root.close()
                }

                Button {
                    text: "Accept"
                    enabled: root.is_valid_shortcut
                    font.pointSize: root.pointSize
                    highlighted: root.is_valid_shortcut
                    onClicked: {
                        root.shortcutAccepted(root.captured_shortcut);
                        root.close();
                    }
                }
            }
        }
    }
}
