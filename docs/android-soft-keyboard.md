# Android / ChromeOS soft keyboard for text inputs

On Android — and especially on ChromeOS running Android apps — a Qt `TextField`
/ `TextArea` does **not** reliably raise the on-screen keyboard when focused or
tapped. Two distinct problems were observed and fixed; both are handled by the
reusable [`MobileKeyboardHelper.qml`](../assets/qml/MobileKeyboardHelper.qml)
component plus a per-field `EnterKey.type`.

## The two problems

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

> ### ⚠ Measured on device 2026-08-08: **Qt 6.10.3 does NOT fix this, and the
> mechanism below is not the whole story.**
>
> Tested with Gboard's Thai layout on a Galaxy S23 (Android 16) against a 6.10.3
> build. Typing **รู้**, the long vowel after the consonant is still reachable
> only with shift-lock — a single Shift tap shows the shifted layer for a moment
> and reverts immediately. **Identical to 6.9.3.**
>
> Three things were established, in order, each disproving the previous
> hypothesis:
>
> 1. **The call-site count was the wrong measurement.** It counts
>    `QtInputConnection.java` only. A `restartImmInput()` call survived in a
>    *different* file, `QtEditText.onKeyDown()`, on the per-keystroke path — and
>    it is **byte-identical in 6.9.3 and 6.10.3**. That looked like a complete
>    explanation, since pressing Shift is itself a key down.
> 2. **Removing that call does not fix the bug.** A patched `QtEditText` was
>    built and verified *live in the dex* (see the trap below), and Thai behaved
>    exactly as before. So `restartImmInput()` is not the cause.
> 3. **No input-connection restart is involved at all.** A logcat capture across
>    a Shift press and a key press shows **zero** `restartInput` /
>    `APP_CALLED_RESTART_INPUT_API`. What it does show is Gboard resetting
>    itself, twice per key press:
>
>    ```
>    LatinIme.resetInputContext(): reason=5,
>        ExternalEditsInfo{action=0, offset=-1, textLength=0,
>                          originalTextLength=0, hasEdits=false}
>    ```
>
>    Note **`textLength=0`** — the editor looks *empty* to the IME even after
>    text has been typed into the QML field.
>
> **Current best hypothesis (unproven):** Qt's Android input connection presents
> a synthetic, essentially empty editor to the IME, keeping the real text in the
> QML item. Gboard cannot reconcile that, resets its input context on each key,
> and a one-shot Shift dies with it. Supporting contrast, from the same device
> and layout: **Firefox behaves correctly** — one Shift tap holds for exactly one
> character, then reverts. So the IME's one-shot mechanism is fine; something in
> Qt cancels it.
>
> **Consequence: upgrading Qt is not a fix for this bug**, and the Thai symptom
> is no longer a reason to upgrade — see
> [android-qt-upgrade-considerations.md §0](./android-qt-upgrade-considerations.md).
> `QT_ANDROID` is back at 6.9.3.
>
> #### Trap for anyone retrying a Qt Java patch
>
> Dropping a patched copy of a Qt class into `android/src/org/qtproject/qt/android/`
> **compiles, packages, and does nothing.** `Qt6Android.jar` still ships Qt's
> version, so the APK defines the class in **two** dex files and ART resolves the
> one in `classes.dex` — Qt's — while the patched copy sits inert in a later dex.
> Measured with `dexdump`: two definitions, only the unpatched one reachable.
>
> To make an override real, the class must be removed from the jar first (a
> Gradle task over `libs/Qt6Android.jar`, hooked to `preBuild`, keeps that inside
> the repo instead of modifying the installed Qt kit). **Verify with `dexdump`,
> never with a green build** — the whole idea was nearly discarded on a false
> negative produced this way. The scaffolding was removed after the experiment;
> the method is recorded here because it is the only way to patch Qt's Android
> Java from this project.
>
> #### Where to pick this up
>
> The next step is **not** another Qt version. It is to confirm the hypothesis
> above — ideally by reproducing the symptom in a minimal Qt Quick app with a
> bare `TextField`, which establishes whether it is Qt-generic or something about
> our fields — and then to file it upstream with the `resetInputContext` /
> `textLength=0` evidence. Ruled out already: `MobileKeyboardHelper`,
> `inputMethodHints`, popup type resolution, `restartImmInput` in both files.

**Status: an upstream Qt bug, not app code. NOT fixed by Qt 6.10.3 (see the box
above); the earlier claim that Qt ≥ 6.10.1 fixes it was inferred from a call-site
count and is now disproven on device.** No app-side fix is currently shipped; the
user workaround is **shift-lock** (double-tap / long-press Shift), which works.

Symptom (Gboard, Thai layout, reported 2026-08-06): Shift gives **one** shifted
character at the start of a word, and **mid-word it is forced straight back to
the base layer** — pressing Shift again does nothing until a space is typed.
Thai's shift layer is a second set of *distinct characters* rather than
capitals, so those characters become untypeable.

The measurements that bound the problem:

| Where | Layout | Mid-word Shift |
|---|---|---|
| Simsapa search field (`SearchBarInput.qml`) | Thai | **forced off** |
| Simsapa Gloss text area (`GlossTab.qml`, `gloss_text_input`) | Thai | **forced off** |
| Simsapa search field | US | works (`dHaMmA` types fine) |
| Firefox search bar | Thai | works (one Shift press per char, repeatable) |

The two Simsapa fields share no configuration — the search field is single-line
with `EnterKey.type: Qt.EnterKeySearch` and an `inputMethodHints`; the gloss
field is a multi-line `TextArea` with neither — and both fail identically. That,
plus the US layout working in the *same* field, puts the fault below QML, in
Qt's Android input-connection layer.

**Eliminated on device — do not re-test:**

1. **`Qt.ImhPreferLowercase`** — removed, no change. Inert on Android
   (`QtEditText.java:37` declares it; nothing reads it). Kept out anyway: Qt
   Virtual Keyboard's `ShiftHandler` does `setShiftActive(!preferLowerCase)`
   (`shifthandler.cpp:280-282`).
2. **`Qt.ImhNoAutoUppercase`** — removed for one build, **no change to Thai**,
   and it cost the lowercase look of romanised queries (the US layout began
   sentence-capitalising, which confirmed the build had taken). **Restored.**
3. **Qt's keyboard-height probe** — `android:windowSoftInputMode="adjustResize"`
   disables `probeForKeyboardHeight()` (armed only when
   `QtInputDelegate.m_softInputMode == 0`, which is what an absent attribute
   gives). No change; reverted. A per-keystroke re-show would also have broken
   the US layout.
4. **`MobileKeyboardHelper`** — present on both failing fields, but it only acts
   on tap/focus, and the failure happens mid-typing with no tap.

**The upstream fix.** qtbase commit
[`f5c0296fdaad`](https://code.qt.io/cgit/qt/qtbase.git/commit/?id=f5c0296fdaad1f4f824e9bd96c525000f658fa81)
— *"Android: Add support for GET_EXTRACTED_TEXT_MONITOR"*, 2025-10-08,
`Fixes:` [QTBUG-140694](https://bugreports.qt.io/browse/QTBUG-140694),
`Task-number:` [QTBUG-138858](https://bugreports.qt.io/browse/QTBUG-138858)
[QTBUG-37980](https://bugreports.qt.io/browse/QTBUG-37980),
`Pick-to: 6.10 6.9 6.8`. It adds
`updateExtractedText()` support so the extracted-text field stays current
**"without the need for restarting the input connection every time input is
given"**, and adds an `m_isComposing` flag so selection changes are ignored
during composition — which it says fixes composition text being corrupted.

That matters here because `InputMethodManager.restartInput()` is precisely what
resets an IME's shift state. The call-site count tells the story:

| Qt version | `restartImmInput()` call sites in `QtInputConnection.java` |
|---|---|
| 6.9.3 (ours) | **12** |
| 6.10.1 | **2** (definition + `sendKeyEvent`) |
| 6.10.3 | **2** — *plus the untouched per-key-down call in `QtEditText.java`* |

**Read that last row before trusting this table.** Counting one file made the
fix look complete; the surviving call in `QtEditText.onKeyDown()` is the one on
the keystroke path, and the device test above shows the symptom is unchanged.

Verified directly: the local 6.9.3 sources contain no
`GET_EXTRACTED_TEXT_MONITOR`, `m_isComposing` or `updateFullScreenExtractedText`;
the 6.10.1 sources contain all three. Qt 6.9.3 was released 2025-09-30 and the
fix landed **eight days later**, so it missed our version by a hair despite
being picked to the 6.9 branch.

Causal story, plausible but **not proven**: mid-word the IME is composing, each
keystroke restarted the input connection, and Gboard rebuilt its keyboard at the
base layer; at a word boundary composition ends, so the next Shift survives.
Why a US layout tolerates the same restarts (manual shift preserved, layout
layer not) is unexplained — do not treat the fix as confirmed until it is tested
on device.

**How to test it:** build the Android target against a Qt ≥ 6.10.1 Android kit
(only `gcc_64` is installed for 6.10.1 — the Android ABIs must be added via the
MaintenanceTool), then re-run the table above. Note the 6.10.1 problems recorded
in [android-qt-upgrade-considerations.md](./android-qt-upgrade-considerations.md)
are **Linux-AppImage-specific** (libtiff SONAME, WebEngine-on-FUSE SIGSEGV) and
do not bear on the Android kits — but the rest of the Android upgrade checklist
in that doc (minSdk 28, AGP/Gradle coupling) does.

### References

- [qtbase `f5c0296fdaad`](https://code.qt.io/cgit/qt/qtbase.git/commit/?id=f5c0296fdaad1f4f824e9bd96c525000f658fa81)
  — the fix, with its commit message.
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
  dated 2025-09-30, eight days before the fix landed.

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
  unresolved Thai Shift bug in §4 — both were removed and re-tested.

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

The helper is mobile-gated (`Qt.platform.os` android/ios) and is a no-op on
desktop, so it is safe to add to any field.

## Where it is applied

Single-line inputs across the app carry `MobileKeyboardHelper {}` +
`EnterKey.type` (search fields: `SearchBarInput`, `ReferenceSearchWindow`,
`TopicIndexWindow`, `WordSummary` lookup → `EnterKeySearch`; form/dialog fields →
`EnterKeyDone`), and the mobile-relevant multi-line content areas
(`ChantingPracticeWindow` Pāli text) carry the helper without an `EnterKey.type`.
Read-only fields and desktop-only config/AI `TextArea`s (e.g. debug output,
keybinding capture, Anki/prompt templates) are intentionally left untouched.

When adding a **new** text input, apply this technique.

## Component registration

`MobileKeyboardHelper.qml` lives in `assets/qml/` and is listed in the
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
