# Android / ChromeOS soft keyboard for text inputs

On Android — and especially on ChromeOS running Android apps — a Qt `TextField`
/ `TextArea` does **not** reliably raise the on-screen keyboard when focused or
tapped, and it misbehaves in several other ways around taps, selection and
window close. The fixed ones are handled by the reusable
[`MobileKeyboardHelper.qml`](../bridges/assets/qml/MobileKeyboardHelper.qml)
component plus a per-field `EnterKey.type` (§5 also needs one line per window);
§4 is a Qt bug fixed by Simsapa's patched Qt for Android kit
([qt-android-patched-kit.md](./qt-android-patched-kit.md)), and
["Tried on device and rejected"](#tried-on-device-and-rejected--do-not-redo)
lists the approaches that were measured and discarded.

## The problems

### 1. The keyboard needs two taps (or never appears)

Symptom: the first tap highlights the field and blinks the cursor (the field
**has** active focus), but the soft keyboard does not come up. A second tap
raises it.

Two compounding causes:

- **A pre-focused field swallows the first tap.** With `focus: true` the field
  already holds active focus by the time the user taps it, so the first physical
  tap is *not* a focus transition — Android's native "focus a field → raise the
  IME" never fires, and `onActiveFocusChanged` never fires either. This is why
  the persistent search bar (`SearchBarInput.qml`) needed two taps. Fix: don't
  pre-grab focus on mobile (`focus: root.is_desktop`); let the first tap be a
  real focus change.

- **A single `Qt.inputMethod.show()` right after a tap/focus is ignored.** The
  focus change has not yet been committed to the platform input context when the
  synchronous call runs, so the request is silently dropped. Fix: request the
  panel on both focus-in and tap, and **retry on a short `Timer`** until
  `Qt.inputMethod.visible` becomes true.

Confirmed from `adb logcat` on the ChromeOS device: on the working single tap,
`request_keyboard` first runs with `activeFocus=false`, then `activeFocusChanged
activeFocus=true` fires, a second `request_keyboard` runs, Qt's
`QtInputDelegate.showKeyboard` drives `InsetsController.show(ime())`, and the
retry observes `im.visible=true` on attempt 1 and stops.

**With the patched Qt kit** (2026-11-25), the dropped `show()` is fixed in Qt
itself (`android/qt-patches/0003`, upstream `5fd393cb69be`, QTBUG-140897: on
Android 16 a keyboard request made twice was cancelled). On the S23 a field
raises the keyboard on the first tap even with the helper's show/retry parts
switched off, and no keyboard flash happens either. **The helper is kept
unchanged anyway** (decision 2026-11-25): its focus-in `show()` is what raises
the keyboard when code focuses a field (`forceActiveFocus()` when a dialog or
window opens; Qt itself raises it only on a press), `0003` acts only on
Android 11+ (minSdk is 28), ChromeOS (where the two-tap logs came from) was
not tested, and iOS needs it. On the patched kit its extra `show()` calls are
harmless. Evidence: `docs/mobile-keyboard-helper.md` in the research repo
(see [qt-android-patched-kit.md](./qt-android-patched-kit.md)).

> **Pitfall — never cast `Qt.inputMethod`.** Access it as an untyped `var`
> (`readonly property var input_method: Qt.inputMethod`). A `Qt.inputMethod as
> InputMethod` cast (added once to silence qmllint) returns **null at runtime on
> some devices** (observed on Samsung/Android 16), so the first
> `input_method.visible` / `.show()` access throws a `TypeError`, aborting
> `request_keyboard()` before `show()` and the retry `Timer` run — which silently
> reintroduces the two-tap bug. The `var` holds the real object on every
> platform and `var` member access isn't type-checked, so qmllint stays quiet.

### 2. The action key does not start the search

Symptom: on first show the keyboard's action key was a generic "Next" arrow that
does **not** emit `accepted`, so `onAccepted` never ran and the query didn't
start. (A later focus showed a "Done" checkmark, which *does* emit `accepted` —
hence the inconsistency.)

Fix: set `EnterKey.type` explicitly. For search fields use
`Qt.EnterKeySearch`, which maps to Android's `IME_ACTION_SEARCH`: a consistent
"search" action key that emits `accepted`. Because the IME action emits
`accepted` (not a physical Return key event), the field's submit logic must live
in `onAccepted`. A field that previously relied on `Keys.onReturnPressed` should
**move** that logic to `onAccepted` (which fires on desktop Return/Enter *and*
the mobile IME action) — do **not** keep both, or a desktop Return can run the
action twice.

### 3. ChromeOS: keyboard suppressed when a hardware keyboard is present

Symptom (reported on a Chromebook running the Android app): tapping the search
field focuses it (`activeFocus=true` in the log) but the soft keyboard **never**
appears, and `Qt.inputMethod.show()` is ignored on every retry
(`inputMethod.visible=false` through all attempts). The user can only enter text
after enabling ChromeOS's on-screen keyboard — and once enabled, even the
**physical** keyboard starts working.

This is **not** an `MobileKeyboardHelper` wiring bug — the diagnostic log proved
the whole chain (platform detection, focus gating, tap → focus, repeated
`show()`) runs correctly. The cause is platform-level, in two layers:

- **Hardware-keyboard IME suppression.** When Android detects a hardware
  keyboard (true on a Chromebook in laptop mode), `showSoftInput()` is a no-op
  unless the secure setting *"Show on-screen keyboard while hardware keyboard is
  connected"* is on. That setting is what the user toggles. An app **cannot**
  override it — `Qt.inputMethod.show()` and the retry `Timer` are powerless here,
  so on ChromeOS the retry loop just spams a call the platform ignores.
- **No input connection until the IME shows.** Qt routes text through the IME's
  `InputConnection`; on ARC that connection isn't active until the IME actually
  comes up, so physical keystrokes also produce no text until the OSK is forced
  on. (Compare the closely related ChromeOS-only Flutter issue #104031: after
  focusing a field, key events aren't delivered to the view.)

We tried adding `android:windowSoftInputMode="adjustResize|stateVisible"` to the
`<activity>` in `android/AndroidManifest.xml` but it did not fix the issue. (It
was tried a second time, as plain `adjustResize`, against the Shift-key bug in
§4 — also no effect. The attribute is **not** set.)

### 4. Mid-word Shift is forced off (Thai and other non-Latin layouts)

**Status: fixed (2026-11-25) by Simsapa's patched Qt for Android kit**, not by
app code and not by a Qt upgrade. The Android build uses that kit by default;
see [qt-android-patched-kit.md](./qt-android-patched-kit.md). With stock Qt
6.9.3 or 6.10.3 the bug is still there, and the user workaround is
**shift-lock** (double-tap or long-press Shift).

Symptom (Gboard, Thai layout, reported 2026-08-06): Shift gives **one** shifted
character at the start of a word, and **mid-word it is forced straight back to
the base layer**. Thai's shift layer is a second set of *distinct characters*
rather than capitals, so those characters (e.g. the long vowel in **รู้**)
could only be typed with shift-lock. The US layout in the same field and
Firefox with the Thai layout both worked.

**Cause** (found with a minimal Qt-only app and logged IME calls, 2026-11-23/24):
Gboard sends the one-shot Shift to the editor as a `KEYCODE_SHIFT_LEFT`
down/up pair through `InputConnection.sendKeyEvent()`, while the current Thai
cluster (e.g. `ร`) is still composing. Qt treated it like any other key:

- `QtInputConnection.sendKeyEvent()` calls `finishComposingText()` for every
  key (an old QTBUG-85715 workaround), modifier keys included. Ending the
  composition is an edit Gboard did not make, so Gboard resets its input
  context (`LatinIme.resetInputContext(): reason=5`) and drops the one-shot
  Shift before the next key.
- In fullscreen (landscape extract) mode, `QtEditText.onKeyDown()` also
  restarts the IME for every key, which drops the Shift the same way.

Android's own `EditText` does neither for a modifier key. The Qt-local patch
`android/qt-patches/0005-…` skips both for `KeyEvent.isModifierKey()`. Three
upstream backports go with it (`0001`–`0003`): without them, landscape
(fullscreen) mode loses composing words and restarts the IME on every edit.
Qt 6.10.3 has two of those backports but not `0005`, which is why it still
failed; Qt `dev` still has both per-key behaviours (read from the source).

What was **wrong** in the earlier analysis here: the `textLength=0` in
Gboard's `ExternalEditsInfo` is only Gboard's summary of the edit (Firefox
logs it too), not a sign that Qt shows the IME an empty editor; and
`restartImmInput()` is a no-op in portrait, non-fullscreen use (it only calls
`restartInput` when Qt is in fullscreen mode), which is why removing it changed
nothing. **Eliminated on device** before the cause was found, still valid:
`Qt.ImhPreferLowercase`, `Qt.ImhNoAutoUppercase`, the keyboard-height probe
(`windowSoftInputMode`), `MobileKeyboardHelper`, popup type resolution.

**Verified on device** (Galaxy S23, Android 16, Gboard 18.6.3) with Simsapa
built on the patched kit, 2026-11-25: one Shift tap mid-word gives the
shifted character, then the base layer, in the search bar and in the gloss
input (`gloss_text_input`); รู้ types without shift-lock; US typing and
suggestions, Backspace over a Thai cluster, Enter, focus switching and rotation
with a composing word all work.

#### Trap for anyone patching Qt's Android Java

Dropping a patched copy of a Qt class into `android/src/org/qtproject/qt/android/`
**compiles, packages, and does nothing.** `Qt6Android.jar` still ships Qt's
version, so the APK defines the class in **two** dex files and ART resolves the
one in `classes.dex` — Qt's. That is why the fix is in the kit's jar, built by
`make android-qt-kit`, and why every package goes through the verify gate
(`scripts/qt-patched-kit/verify-package.sh`, which counts the class definitions
with `dexdump` and looks for the fix's call sites). **Verify with `dexdump`,
never with a green build.**

### 5. The blue cursor handle stays on screen after a window closes

Symptom (Android 16 device, Qt 6.9.3): tap in a `TextArea` to place the cursor,
then close the window with Back. The window goes, but Android's blue teardrop
cursor handle stays painted over the app until it is restarted.

The handle is a Java view, shown and hidden only from
`QAndroidInputContext::updateSelectionHandles()`
(`qandroidinputcontext.cpp:596`), which gives up at
`if (noHandles || !m_focusObject) return;`. Closing a window destroys the focus
object, so nothing hides the handle. **The leak needs the window to be
destroyed** — it is not a `TextArea` trait and not a focus-change failure: in
the Gloss tab, switching tabs or toggling the sidebar hides the handle every
time, because that window stays alive and focus lands on another item.

**The fix: leave edit mode at the top of the window's own `onClosing`**, before
anything notifies the `WindowManager` — `ChantingPracticeReviewWindow.qml` is
the worked example. It does exactly what unchecking its Edit button does:

```qml
onClosing: function(close) {
    if (pali_edit_button.checked || pali_text.activeFocus) {
        pali_edit_button.checked = false;      // readOnly binding follows
        pali_text.activeFocusOnPress = false;
        pali_text.focus = false;
    }
    // ...the rest of the close handling
}
```

Both halves matter. Clearing focus alone is **measured not to work** (below), so
the read-only half is doing work: a read-only field with no selection takes a
different branch of `updateSelectionHandles()` than the focus one. A field with
no edit mode would set `readOnly = true` directly — but only if nothing *binds*
`readOnly`, since the assignment would replace the binding.

**Two earlier shapes failed on device. Do not re-attempt them:**

1. `MobileKeyboardHelper` cleared focus on the window's `closing` signal (two
   variants: focus to the field's parent, then to a zero-sized `Item` child of
   the helper). The log proves it ran and that focus moved
   (`field.activeFocus=false sink.activeFocus=true`) — and the handle stayed.
   That signal arrives **after** the window's own `onClosing` has notified the
   `WindowManager` (`on_window_closed(chanting_review): destroyed` is logged
   *before* the helper's line), so a helper-side fix cannot be early enough.
   **This is why the fix belongs in each window, not in the shared helper.**
2. `ChantingPracticeReviewWindow` moved focus to a plain `Button` as the first
   statement of its own `onClosing` — early enough, but focus-only. It made
   things **worse**: Qt repositioned the handle to the top-left corner instead
   of hiding it.

**Other windows that are destroyed on close and host a text field have not been
checked** and presumably still leak. Apply the pattern above when one is
reported; the pooled reader window is not affected, since it is only hidden.

### References

(§4.) The full list of patches, with their upstream commits and bugs, is in
[qt-android-patched-kit.md](./qt-android-patched-kit.md) §2.

- [qtbase `f5c0296fdaad`](https://code.qt.io/cgit/qt/qtbase.git/commit/?id=f5c0296fdaad1f4f824e9bd96c525000f658fa81)
  — once taken for "the fix"; it only covers fullscreen mode, and is carried
  as the backport `android/qt-patches/0002`.
- [`QtInputConnection.java` history](https://code.qt.io/cgit/qt/qtbase.git/log/src/android/jar/src/org/qtproject/qt/android/QtInputConnection.java)
  — how the `restartImmInput()` call sites evolved. The file at a given release
  can be read directly, which is how the 12-vs-2 count was verified:
  `…/plain/src/android/jar/src/org/qtproject/qt/android/QtInputConnection.java?h=v6.10.1`
- [QTBUG-140694](https://bugreports.qt.io/browse/QTBUG-140694) — the bug the fix
  closes.
- [QTBUG-138858](https://bugreports.qt.io/browse/QTBUG-138858),
  [QTBUG-37980](https://bugreports.qt.io/browse/QTBUG-37980) — the tasks it
  advances.
- [QTBUG-68822](https://bugreports.qt.io/browse/QTBUG-68822) — "QAndroidInputContext:
  Improve compatibility with virtual keyboards", the umbrella issue for this
  class of problem.
- [QTBUG-59958](https://bugreports.qt.io/browse/QTBUG-59958) — Gboard/SwiftKey
  predictive input corrupting text; same layer, same era.
- [Qt 6.9.3 release announcement](https://www.qt.io/blog/qt-6.9.3-released) —
  dated 2025-09-30, eight days before `f5c0296fdaad` landed.

> Practical note: `bugreports.qt.io/browse/…` now 301-redirects to
> `qt-project.atlassian.net`, which renders through JavaScript — so these
> tickets **cannot be read by command-line fetchers**. Open them in a browser.
> The cgit links above are plain HTML and work fine from a terminal, which is
> why the analysis in this section is built on source and commit messages rather
> than on ticket text.

## How to apply it

Drop `MobileKeyboardHelper {}` as a child of the input — with no arguments it
targets its parent field — and set an appropriate `EnterKey.type`:

```qml
// A search / lookup field whose action key should run the search:
TextField {
    id: search_input
    inputMethodHints: Qt.ImhNoAutoUppercase   // no Sentence-case; see below
    EnterKey.type: Qt.EnterKeySearch
    onAccepted: search_btn.clicked()   // the IME search action emits `accepted`
    MobileKeyboardHelper {}
}

// A normal form field (commit & dismiss):
TextField {
    id: title_field
    EnterKey.type: Qt.EnterKeyDone
    MobileKeyboardHelper {}
}

// A multi-line field — do NOT set EnterKey.type (Enter inserts a newline):
TextArea {
    id: body_field
    wrapMode: TextEdit.WordWrap
    MobileKeyboardHelper {}
}
```

Guidelines:

- **`EnterKey.type`**: `Qt.EnterKeySearch` for search/lookup inputs;
  `Qt.EnterKeyDone` for single-line form fields; **omit** it for multi-line
  `TextArea`s (Enter must insert a newline).
- **`onAccepted`**: any field with `EnterKey.type` that triggers an action must
  handle `onAccepted` (the IME action does not produce a Return key event). If
  the field only had `Keys.onReturnPressed`, **move** that logic to `onAccepted`
  rather than keeping both (a desktop Return could otherwise run it twice).
- **Pre-focused persistent fields** (a search bar that sits focused): gate
  auto-focus to desktop (`focus: root.is_desktop`) so the first mobile tap is a
  real focus transition. Modal dialog fields that gain focus when the dialog
  opens do not need this — the open *is* the focus transition.
- **`inputMethodHints`: `Qt.ImhNoAutoUppercase` only, never
  `Qt.ImhPreferLowercase`.** `ImhNoAutoUppercase` suppresses the IME's
  Sentence-case auto-capitalisation, which keeps romanised queries looking
  lowercase — a cue that search is case-insensitive. `ImhPreferLowercase` asks
  the IME to sit on its lowercase layer, which is meaningless for non-Latin
  scripts whose shift layer holds distinct characters; it is inert on Android
  but not under Qt Virtual Keyboard. Neither hint is responsible for the
  Thai Shift bug in §4 (fixed by the patched Qt kit) — both were removed and
  re-tested.

  Case-insensitivity of queries is a **backend** guarantee, not an IME one, so
  nothing is lost: `SearchQueryTask::new()` lowercases every mode
  (`UidMatch` → `to_lowercase()`; `FulltextMatch` → `normalize_fulltext_query`;
  everything else → `normalize_query_text` → `compact_plain_text`, both of which
  route through `normalize_plain_text`, whose first step is `to_lowercase()`),
  and the DPD paths (`dpd_lookup`, `dpd_lookup_grouped`) normalize their query
  text and lower-case the uid candidate. Do not re-add case handling in QML.
- **`gesturePolicy`**: the helper's `TapHandler` must stay `DragThreshold` (its
  default). That gives it a *passive* grab so taps still reach the field for
  cursor placement and text selection; a drag past the threshold cancels the tap.
  Never change it to `WithinBounds`/`ReleaseWithinBounds` — those take an
  exclusive grab and swallow the cursor tap.

- **A tap on an already-focused field with the keyboard up requests nothing.**
  On Android each `show()` re-runs `QtInputDelegate.showSoftwareKeyboard()`, so
  redundant requests on a cursor-moving tap make the keyboard flash off and on.
  The `TapHandler` samples `activeFocus` on press (before the field takes focus)
  and skips only when the field already had focus **and**
  `Qt.inputMethod.visible` is true. **Never apply that `visible` check to the
  focus-in path or the retry `Timer`**: right after a focus change it can read
  true while the keyboard is not up, and skipping there brings back the two-tap
  bug.
- **Qt's own per-press `show()` is suppressed for that same tap.**
  `QQuickTextInput`/`QQuickTextEdit::mousePressEvent` call
  `QInputMethod::show()` on every press on an already-focused field
  (`focusOnPress && hadActiveFocus`), and on Android that alone flashes the
  keyboard once. Pointer handlers receive the press before the item, so the
  helper's `TapHandler` sets the field's `activeFocusOnPress = false` on press
  (same condition as above) and restores it with `Qt.callLater` after release.
  The cursor still moves and focus is kept. Because this is an assignment,
  **a field using the helper must not bind `activeFocusOnPress`** — set it
  imperatively (see `apply_pali_edit_mode()` in
  `ChantingPracticeReviewWindow.qml`). The restore is skipped when the field has
  become `readOnly` during the tap, so a mode toggle or a closing window is not
  handed back a focus-on-press it deliberately turned off.
- **Long-press selection in a `TextArea` is re-applied after release.** The
  Controls press handler delays the press and drops it once the press-and-hold
  interval expires, so `QQuickTextControl` never sees a press, the long-press
  selection does not set `imSelectionAfterPress`, and the touch release calls
  `setCursorPosition()` — the highlight vanishes and the cursor jumps to where
  the finger lifted. (`TextField` guards this with `hasSelectedText()`.) The
  helper's `TapHandler` sees the release first: a selection that did not exist
  at press is saved and restored with `Qt.callLater` via `select()`.

The helper is mobile-gated (`Qt.platform.os` android/ios) and is a no-op on
desktop, so it is safe to add to any field.

## Tried on device and rejected — do not redo

Each of these was written, built and measured on an Android 16 device, on
**stock** Qt 6.9.3. The flash they deal with came from stock Qt restarting
Gboard's input view on every `show()`; the patched kit removes that, but the
helper keeps its flash handling for stock builds and other platforms. They are
listed so the same ground is not covered twice; the two failed shapes of the §5
cursor-handle fix are recorded there rather than repeated here.

- **Skipping `show()` whenever `Qt.inputMethod.visible` is true**, in
  `request_keyboard()` and in the retry `Timer`. Intended to stop the
  keyboard-flash; it **reintroduced the two-tap bug** — right after a focus
  change Qt can report the keyboard visible while it is not up, so the first tap
  raised nothing. The `visible` check is only safe for a tap on a field that
  *already had focus at the press*, which is where it now lives.
- **Relying on the helper alone to stop the flash.** Skipping the helper's
  redundant requests cut the flashing from several per tap to exactly one, and
  no further: the remaining one is Qt's own `show()` from
  `QQuickTextInput`/`QQuickTextEdit::mousePressEvent`. Suppressing
  `activeFocusOnPress` for that tap is what removed it. Measured in that order —
  do not expect either half to be sufficient alone.
- **Reading the keyboard/handle diagnostics from `logger.debug()`.** They never
  reach logcat or `log.txt`: `backend/src/logger.rs` fixes the tracing filter at
  `info` when the app starts, and the runtime log-level setting does not lift
  it. Any device diagnostic in QML has to be `logger.info()`.

## Where it is applied

Single-line inputs across the app carry `MobileKeyboardHelper {}` +
`EnterKey.type` (search fields: `SearchBarInput`, `ReferenceSearchWindow`,
`TopicIndexWindow`, `WordSummary` lookup → `EnterKeySearch`; form/dialog fields →
`EnterKeyDone`), and the mobile-relevant multi-line content areas
(`ChantingPracticeWindow` Pāli text, the gloss inputs, the recording marker
comment) carry the helper without an `EnterKey.type`.
Read-only fields and desktop-only config/AI `TextArea`s (e.g. debug output,
keybinding capture, Anki/prompt templates) are intentionally left untouched.

**Known limitation (QTBUG-151926):** a long-press selection in a `TextArea`
**without** the helper is lost when the finger lifts (the helper's re-apply is
the workaround). That applies to the 13 read-only `TextArea`s (AI responses,
update notes, diagnostics, …); selecting with the handles still works there.
Left as is on purpose (2026-11-25). If the helper's re-apply ever stops
working, the fallback is a plain `TextEdit` in a `ScrollView` instead of a
Controls `TextArea` (it kept the selection 6/6 on the S23); the research
repo's `cxx-qt-prototype/qml/PlainTextEditArea.qml` is a ready component. No
press-and-hold interval change is needed: with the helper, a long press keeps
its selection in `TextField`s and `TextArea`s at Qt's default interval.

When adding a **new** text input, apply this technique.

## Component registration

`MobileKeyboardHelper.qml` lives in `bridges/assets/qml/` and is listed in the
`qml_files` array in `bridges/build.rs`. New QML components must be added there
(see AGENTS.md → "New QML components").

## References

ChromeOS hardware-keyboard suppression / input-connection issue (section 3):

- [Flutter #104031 — ChromeOS: after focusing a text field, no printable key
  events are received](https://github.com/flutter/flutter/issues/104031) — same
  class of bug in another framework; ARC intercepts key events after focus.
- [ChromeOS input compatibility](https://chromeos.dev/en/android/input-compatibility)
  — how ChromeOS handles text input / IME for Android apps.
- [Android — Handle input method visibility](https://developer.android.com/develop/ui/views/touch-and-input/keyboard-input/visibility)
  — `showSoftInput()` semantics and window-focus requirements.

Qt soft-keyboard handling on Android (sections 1–2):

- [Qt Forum — Android virtual keyboard input trouble](https://forum.qt.io/topic/46231/android-virtual-keyboard-input-trouble)
- [Qt Forum — Qt 6.7.1 / 6.6.1 Android soft keyboard handling](https://forum.qt.io/topic/157124/qt-6-7-1-and-qt-6-6-1-android-softkeyboard-handling)
