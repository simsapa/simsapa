import QtQuick

// Reusable Android/ChromeOS soft-keyboard activator.
//
// Drop it as a child of any TextField / TextArea — with no arguments it targets
// its parent input field:
//
//     TextField {
//         id: my_field
//         MobileKeyboardHelper {}
//     }
//
// Rationale and the full story are in docs/android-soft-keyboard.md. In short:
// on Android (and especially ChromeOS running Android apps) a single
// Qt.inputMethod.show() issued right after a tap/focus is often ignored,
// because the focus change has not yet been committed to the platform input
// context — which is why the search bar needed a second tap. This helper
// requests the panel both on focus-in and on tap, and retries on a short Timer
// until the IME reports visible.
Item {
    id: helper

    Logger { id: logger }

    // The input field to drive. Defaults to the parent element so the helper
    // can be dropped inside a TextField/TextArea with no arguments.
    property Item field: parent

    // `activeFocusOnPress` exists on TextInput/TextEdit but not on Item, so
    // access it through an untyped alias to keep qmllint quiet.
    readonly property var text_field: helper.field

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"

    // Qt.inputMethod is typed as the base QObject in the QML global object, so
    // the linter reports its show()/visible members as missing. Expose it as an
    // untyped `var` so the calls below are not type-checked.
    //
    // Do NOT use `Qt.inputMethod as InputMethod`: that cast returns **null** at
    // runtime on some devices (observed on Samsung/Android 16), so every
    // `input_method.show()` / `.visible` access throws a TypeError. That aborted
    // request_keyboard() before show()/the retry Timer ran and reintroduced the
    // two-tap bug. A plain `var` holds the real object on every platform.
    readonly property var input_method: Qt.inputMethod

    // Keyboard diagnostics: confirm the helper is active and which platform it
    // sees. If is_mobile is false on a Chromebook, the Connections/TapHandler
    // below are disabled and the keyboard is never requested.
    Component.onCompleted: logger.debug("MobileKeyboardHelper: Qt.platform.os="
        + Qt.platform.os + " is_mobile=" + helper.is_mobile
        + " field=" + helper.field)

    // Zero-size: this is a behaviour helper, not a visual element.
    width: 0
    height: 0

    // A single Qt.inputMethod.show() right after a tap/focus is unreliable on
    // Android/ChromeOS — the focus change may not be committed to the platform
    // input context yet, so the request is silently dropped. Retry on a short
    // Timer until the IME reports visible (or we give up after a few tries).
    Timer {
        id: retry_timer
        interval: 150
        repeat: true
        property int attempts: 0
        onTriggered: {
            attempts += 1;
            helper.input_method.show();
            logger.debug("MobileKeyboardHelper: retry attempt=" + attempts
                + " inputMethod.visible=" + helper.input_method.visible);
            if (helper.input_method.visible || attempts >= 5) stop();
        }
    }

    function has_selection(): bool {
        return helper.text_field !== null
            && helper.text_field.selectionStart !== helper.text_field.selectionEnd;
    }

    function request_keyboard() {
        logger.debug("MobileKeyboardHelper: request_keyboard() called, "
            + "inputMethod.visible=" + helper.input_method.visible);
        helper.input_method.show();
        retry_timer.attempts = 0;
        retry_timer.restart();
    }

    // Field gained active focus (tapped, or a dialog opened onto it): request
    // the keyboard.
    Connections {
        target: helper.field
        enabled: helper.is_mobile && helper.field !== null
        function onActiveFocusChanged() {
            logger.debug("MobileKeyboardHelper: field.onActiveFocusChanged activeFocus="
                + helper.field.activeFocus);
            if (helper.field.activeFocus) helper.request_keyboard();
        }
    }

    // Re-tapping an already-focused field produces no focus-change signal, so
    // also request the keyboard on a tap — but only when it can be needed.
    //
    // A tap on a field that already had focus while the keyboard is up (e.g.
    // moving the cursor) must NOT request it: on Android each show() re-runs
    // QtInputDelegate.showSoftwareKeyboard(), and the extra requests make the
    // keyboard flash off and on. Do not move this `visible` check into
    // request_keyboard(): right after a focus change it can read true while the
    // keyboard is not up, and skipping there brings back the two-tap bug. The
    // focus-in path above must stay unconditional.
    //
    // `had_focus_at_press` is sampled on press, which reaches a pointer handler
    // before the field's own mousePressEvent takes focus.
    //
    // Qt itself also re-requests the keyboard on every press on a focused field
    // (QQuickTextInput/QQuickTextEdit mousePressEvent: `focusOnPress` +
    // `hadActiveFocus` → QInputMethod::show()), which flashes the keyboard once.
    // For that same tap the handler turns the field's `activeFocusOnPress` off
    // — handlers see the press first — and turns it back on after the
    // release. The cursor still moves (that does not depend on focusOnPress)
    // and the field keeps its focus. This is an imperative assignment, so a
    // field using the helper must not *bind* `activeFocusOnPress`: the restore
    // would replace the binding with a constant.
    //
    // Long-press word selection is also kept here, for TextArea. The Controls
    // press handler holds the press back until the press-and-hold interval
    // expires and then drops it, so QQuickTextControl never sees the press,
    // the long-press selection from QAndroidInputContext::longPress() does not
    // set `imSelectionAfterPress`, and the release calls setCursorPosition():
    // the selection vanishes and the cursor jumps to where the finger lifted
    // (qquicktextcontrol.cpp mouseReleaseEvent). TextInput guards this with
    // hasSelectedText() and is unaffected. The handler sees the release first,
    // so a selection that did not exist at press is saved and re-applied once
    // the release has been delivered.
    //
    // gesturePolicy MUST stay DragThreshold (the default): the handler then
    // takes only a PASSIVE grab, so the press/move/release still reach the
    // underlying input — tap-to-position-cursor, drag-to-select and the
    // selection handles keep working, and a drag past the threshold cancels the
    // tap so it never competes with text selection. Do NOT change this to
    // WithinBounds/ReleaseWithinBounds — those take an exclusive grab and would
    // swallow the cursor tap.
    TapHandler {
        parent: helper.field
        enabled: helper.is_mobile && helper.field !== null
        gesturePolicy: TapHandler.DragThreshold
        property bool had_focus_at_press: false
        property bool suppressed_focus_on_press: false
        property bool had_selection_at_press: false
        onPressedChanged: {
            if (pressed) {
                had_focus_at_press = helper.field.activeFocus;
                had_selection_at_press = helper.has_selection();
                if (had_focus_at_press && helper.input_method.visible
                        && helper.text_field.activeFocusOnPress) {
                    helper.text_field.activeFocusOnPress = false;
                    suppressed_focus_on_press = true;
                }
            } else {
                if (suppressed_focus_on_press) {
                    // Deferred, so a platform that focuses on touch *release*
                    // has already handled the release before the value comes back.
                    suppressed_focus_on_press = false;
                    // Not if the field went read-only in the meantime (a mode
                    // toggle, a closing window): turning focus-on-press back on
                    // there would undo exactly what read-only mode is for.
                    Qt.callLater(() => {
                        if (helper.text_field && !helper.text_field.readOnly) {
                            helper.text_field.activeFocusOnPress = true;
                        }
                    });
                }
                if (!had_selection_at_press && helper.has_selection()) {
                    const start = helper.text_field.selectionStart;
                    const end = helper.text_field.selectionEnd;
                    Qt.callLater(() => { if (helper.text_field) helper.text_field.select(start, end); });
                }
            }
        }
        onTapped: {
            logger.debug("MobileKeyboardHelper: TapHandler onTapped had_focus_at_press="
                + had_focus_at_press + " inputMethod.visible=" + helper.input_method.visible);
            if (had_focus_at_press && helper.input_method.visible) return;
            helper.request_keyboard();
        }
    }
}
