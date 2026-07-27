# PRD — Android API 36 compliance and packaging follow-ups

**Created:** 2026-07-27
**Status:** Draft — not yet implemented
**Supersedes:** `tasks/android-packaging-follow-ups.md` (its diagnosis and reasoning
are folded into this document; that file is already gone — it was never committed)

Related docs:
- [docs/android-multi-abi-and-chromeos.md](../docs/android-multi-abi-and-chromeos.md)
- [docs/app-packaging-and-identifiers.md](../docs/app-packaging-and-identifiers.md)
- [docs/android-soft-keyboard.md](../docs/android-soft-keyboard.md)
- [docs/pure-rust-audio-backend.md](../docs/pure-rust-audio-backend.md)
- [docs/qt-6.10.1-appimage-issues.md](../docs/qt-6.10.1-appimage-issues.md)

---

## 1. Introduction / Overview

Google Play has told us that our highest non-compliant target API level is
Android 15 (API 35) and that we must target Android 16 (API 36) or higher to
keep publishing updates. Play additionally reports that the app calls three
deprecated window APIs (`Window.getStatusBarColor`, `Window.setStatusBarColor`,
`Window.setNavigationBarColor`).

Raising `targetSdkVersion` is a one-line change; **the behaviour changes it
switches on are not.** Targeting API 36 makes three system behaviours
unconditional, none of which the app currently accounts for:

1. **Edge-to-edge is enforced with no opt-out.** The window draws behind the
   status and navigation bars. The `windowOptOutEdgeToEdgeEnforcement` escape
   hatch that exists at targetSdk 35 is ignored at 36.
2. **Predictive back is on by default.** `onBackPressed()` is no longer called
   and `KEYCODE_BACK` is no longer dispatched.
3. **Large-screen orientation/resizability attributes are ignored** on displays
   with smallest width ≥ 600dp — which includes the Chromebooks we just did
   work to support.

This PRD covers that compliance work on **Qt 6.9.3** (no Qt upgrade), plus the
deferred Android packaging items that were parked when the multi-ABI
Chromebook-compatible build shipped in July 2026.

The goal is a signed multi-ABI App Bundle that targets API 36, looks and
behaves on a phone the way the current build does, and can be uploaded to Play
without a compliance warning — plus the packaging hygiene that makes the next
upload less error-prone.

---

## 2. Goals

1. The published AAB declares `targetSdkVersion 36` and is accepted by Play
   with no target-API compliance warning.
2. On an Android 16 device nothing is clipped or unreachable — no toolbar under
   the status bar, no button under the gesture bar — in portrait and landscape,
   with and without the soft keyboard. The one deliberate visual *change* is
   that the doubled top gap becomes a single inset.
3. Back navigation behaves the same as today (verified on device; opt out of
   predictive back if it does not).
4. Both Android version values are read by the build script from single-purpose
   files (`android/version.txt`, `bridges/Cargo.toml`) and passed to CMake via
   the environment; a release build needs no version arguments, and every build
   states the values it used and where they came from.
5. Release builds stop building and signing a throw-away debug bundle.
6. Every remaining "unverified but probably fine" packaging item is either
   verified or explicitly closed with a recorded decision.
7. The reasons and pitfalls for a **future** Qt upgrade are written down while
   the analysis is fresh, so the migration is a planned task and not a
   discovery exercise.

---

## 3. User Stories

- **As an Android user**, I want to keep receiving app updates from the Play
  Store, so the app must stay compliant with Play's target-API policy.
- **As an Android 16 user**, I want the search bar, toolbars and buttons to be
  fully visible and tappable — not tucked under the status bar or the gesture
  navigation bar.
- **As a user with an older 32-bit phone (armeabi-v7a)**, I want the app to
  keep working; we have such users and are not dropping that ABI.
- **As a Chromebook user**, I want the app to keep being offered to my device
  and to resize sensibly in a window.
- **As the maintainer**, I want the build to stop me from uploading a bundle
  with a versionCode Play will reject, instead of finding out after a 3× ABI
  build has finished.
- **As the maintainer**, I want a written record of *why* each Android
  packaging decision is what it is, so a future Qt or AGP bump does not undo it
  by accident.

---

## 4. Functional Requirements

### A. Target API 36

1. `android/build.gradle` `defaultConfig` must set `targetSdkVersion 36`
   (currently 35). `minSdkVersion` stays **27**.
2. The change must be accompanied by a comment recording that API 36 enforces
   edge-to-edge with no opt-out, enables predictive back by default, and
   ignores fixed-orientation attributes on large screens — so a future reader
   knows the number is load-bearing.
3. `compileSdk` continues to come from androiddeployqt's generated
   `gradle.properties` (`androidCompileSdkVersion=android-36`, picked from the
   newest installed platform). No change; see requirement 39.

   Confirmed that requirement 1 is the *whole* change: `android/AndroidManifest.xml`
   has no `<uses-sdk>` element and `CMakeLists.txt` sets no
   `QT_ANDROID_TARGET_SDK_VERSION`, so `build.gradle`'s `defaultConfig` is the
   only source of the target level.

   **Red herring during verification:** androiddeployqt also writes
   `qtTargetSdkVersion=35` into the generated `gradle.properties`, and
   `build.gradle` never reads it (it hardcodes `targetSdkVersion`). That line
   will still say `35` after this change and means nothing. Verify with
   `aapt2 dump badging` on the built artifact (requirement 5 / task 5.7), not by
   reading generated properties.
4. A signed multi-ABI AAB must still build end-to-end with the existing
   toolchain: Qt 6.9.3, NDK 27.3.13750724, JDK 21, AGP 8.6.0, Gradle wrapper
   8.12, build-tools 36.0.0.
5. The build must still pass the existing artifact checks in
   `build-android.sh` (no cross-ABI staged libraries, correct ELF machine
   types, `zipalign -P 16`).

### B. Edge-to-edge: let Qt own the inset, keep one *extra* top-margin knob

Confirmed on device: in the current build the space above the search bar is
roughly **twice** the status-bar height — Qt's automatic `ApplicationWindow`
padding *and* our manual margin, both applied (§6.1). The system inset is
therefore already handled correctly, per window, live, cutout-aware. What
remains is a user escape hatch for the case that made the setting necessary in
the first place.

6. On Android, window content must not be obscured by the status bar, the
   navigation/gesture bar, or a display cutout, in **both** orientations. Every
   button, tab and list row must remain visible and tappable.
7. **The system inset is Qt's job — do not re-derive it.** `ApplicationWindow`
   binds its padding to the window's safe area
   (`qquickapplicationwindow.cpp:801-805`), which is the union of the system
   bars and the display cutout, updated live and resolved per window. The app
   must not compute, cache or plumb it (§6.3).
8. `MobileTopBarMargin` must be replaced by a **single plain integer** setting —
   an *extra* top margin applied inside Qt's padding — defaulting to **0**:

   ```rust
   /// Extra space (dp) added below the system-provided safe area at the top of
   /// mobile windows. 0 = rely on the system inset alone.
   pub mobile_extra_top_margin: u32,
   ```

   (`AppSettings` already carries a **container-level** `#[serde(default)]` at
   `app_settings.rs:161`, so a per-field `#[serde(default)]` is redundant.)

   The `SystemValue` / `CustomValue` distinction disappears: the "system value"
   is now supplied by Qt, and this setting is only ever the amount added on top.
9. **Migration is a straight value carry-over, with no visual change for anyone
   who had customised it:**

   | Old setting | New value | Effect |
   |---|---|---|
   | `SystemValue` (the default) | `0` | The doubled gap disappears — this is the fix |
   | `CustomValue(v)` | `v` | Identical to what that user sees today |

   This works because the old value was *already* being applied on top of Qt's
   padding on any Qt 6.9.3 build; only the default was wrong.

   **The migration must not be a call-site hook.** `AppSettings` is
   deserialized in **three** places, not one (§6.7):
   `backend/src/db/appdata.rs:443` (the in-app path that fills
   `app_settings_cache`), `backend/src/db/mod.rs:389` (the standalone
   pre-`QApplication` read) and `backend/src/app_data.rs:3237` (the
   `import-me/app_settings.json` import). A hook added to one of them silently
   skips the others. Implement it as `#[serde(from = "…")]` (or a manual
   `Deserialize`) on `AppSettings` itself, so no reader can bypass it and a
   fourth reader added later inherits it.
10. The `status_bar_height` JNI path is no longer part of the margin
    resolution. `SuttaBridge.get_status_bar_height()` / `cpp/utils.cpp:30` may
    be kept **only** to display the value for information; if nothing else uses
    it, delete it.
11. The Settings section must be relabelled to match the new meaning — e.g.
    **"Extra Top Margin"** with the explanation that the system status bar and
    camera cutout are already accounted for automatically, and that this adds
    space below them if the app's top elements are still covered on a
    particular device. The "use system value" checkbox is removed; what remains
    is one SpinBox, default 0, `visible: root.is_mobile`.
12. Settings should display the **live** system inset alongside the SpinBox as
    read-only information (e.g. "System safe area: 34 dp"), so a user
    troubleshooting a covered toolbar can see what the platform reported.
13. The mechanism for *applying* the value stays as it is today — an anchor
    margin on the window's root layout, inside Qt's padding, and passed to the
    child windows — since that is precisely "extra space in addition to the
    safe area". No new QML helper component is needed.
14. **Do not add bottom / left / right knobs.** Qt pads all four edges
    automatically; there is no evidence of an edge other than the top being
    under-reported, and four independent overrides are four ways to break the
    layout. If the on-device pass (§9) finds a real problem on another edge, add
    that edge then, following the same pattern.
15. **The cases Qt does not pad automatically must be audited and handled
    explicitly** (§6.3): `ApplicationWindow`'s `header` / `footer` / `menuBar`
    (siblings of the contentItem, deliberately unpadded by Qt), inline
    `Dialog` / `Popup` items (positioned in the window overlay, not the
    contentItem), and `Flickable`/`ListView` content that scrolls under an edge.
    **15a — `assets/qml/DrawerMenu.qml` is a known instance of that gap and must
    be fixed as part of this change, not deferred to the device pass.** Its root is
    a `Drawer` (a `Popup` subclass) with `height: control.window_height`, so it
    is positioned in the window overlay and receives none of Qt's padding, and
    its first child is a `Label { text: "Menu" }`. Under enforced edge-to-edge
    that label sits under the status bar / cutout. It is the **mobile main
    menu** (instantiated at `SuttaSearchWindow.qml:2249`), so this is the most
    user-visible unpadded surface in the app. Give the `Drawer` its own
    `topPadding: SafeArea.margins.top` (attached to the drawer itself, so the
    value is relative to it) rather than plumbing a number in.
16. The WebEngine/WebView reader panels must not render content under the
    system bars; the HTML viewport must sit inside the padded content area.
17. **Verify that Qt keeps reporting non-zero safe-area margins at
    targetSdk 36.** They demonstrably arrive today at targetSdk 35 (that is what
    the doubled gap proves), and nothing in the API 36 change should alter it —
    but it is one observation and it underpins the whole design, so it must be
    re-checked on the first targetSdk 36 build rather than assumed.
18. `assets/qml/MobileTopMarginDialog.qml` is **dead code** — referenced by no
    QML file and absent from `bridges/build.rs`'s `qml_files`. Delete it.
19. The bridge API must be renamed to match the new setting
    (`get_mobile_extra_top_margin` / `set_mobile_extra_top_margin`), with the
    old `*_mobile_top_bar_margin*` methods removed. Matching stubs must be added
    to `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` per the project
    rule.
20. This fix is **not specific to targetSdk 36**: the doubled gap exists in the
    shipped build on every Android 15+ device, and on older devices the old
    default added a wasted `status_bar_height` gap below an already-letterboxed
    window. Defaulting to 0 improves both.

### C. Predictive back

21. Back navigation must be tested on device with `targetSdkVersion 36`
    **before** deciding the implementation: verify the system back gesture and
    hardware back button in the main window, in dialogs/popups, and in the
    secondary windows.
22. If back behaviour regresses (for example the gesture closes the app instead
    of dismissing a dialog), the app must opt out for this release by adding
    `android:enableOnBackInvokedCallback="false"` to the `<activity>` element in
    `android/AndroidManifest.xml`, with a comment recording that the opt-out is
    temporary and disappears at a future API level.
23. The test result and the decision must be recorded in the docs, whichever
    way it goes.

### D. Large screens / Chromebook

24. Confirm the app still behaves when fixed-orientation and aspect-ratio
    attributes are ignored (smallest width ≥ 600dp). The manifest already uses
    `android:screenOrientation="unspecified"` and declares
    `resizeableActivity`-friendly `configChanges`, so no change is expected —
    but this must be **verified**, not assumed, and the finding recorded.
25. Do **not** add
    `android.window.PROPERTY_COMPAT_ALLOW_RESTRICTED_RESIZABILITY` unless
    verification shows a concrete problem; it is a temporary opt-out that stops
    working at API 37.

### E. Deprecated window colour APIs

26. No app-side code change. The three reported calls are inside **Qt's own
    Java** (see §7.3) and cannot be removed without changing Qt. The
    requirement is to **document** this: the calls are no-ops at API 36, the
    Play report is informational, and the removal path is the future Qt
    upgrade.
27. Record the finding in `docs/android-multi-abi-and-chromeos.md` (or the new
    Android edge-to-edge doc) with the exact file/line references, so the next
    Play report does not trigger a fresh investigation.

### F. App version single source of truth

28. The two version values are **parsed by `build-android.sh`** from the files
    that are hand-edited anyway, exported as environment variables, and picked
    up by `CMakeLists.txt`. Nothing carries a second copy:

    | Value | Read from | Edited by hand |
    |---|---|---|
    | `ANDROID_VERSION_CODE` | `android/version.txt` | yes — bump before a release upload |
    | `ANDROID_VERSION_NAME` | `bridges/Cargo.toml` `version` | yes — already bumped every release |

29. Add a tracked file **`android/version.txt`** holding just the versionCode:

    ```
    # Android versionCode. Google Play requires this to strictly increase on
    # every upload. The last uploaded value is visible in the Play Console.
    3
    ```

    Blank lines and `#` comments ignored; the first remaining line must be a
    positive integer. Seed it with **`3`** — Play currently has 2. There is no
    `last_uploaded` bookkeeping: Play rejects a duplicate at upload time and the
    Console shows the last accepted value.
30. `build-android.sh` must, before configuring:
    - read `android/version.txt` → `ANDROID_VERSION_CODE`;
    - read the package `version` from `bridges/Cargo.toml` →
      `ANDROID_VERSION_NAME`;
    - **export** both so CMake sees them;
    - fail with an actionable message if either file is missing or unparseable,
      or if the versionCode is not a positive integer.

    Parse both with a targeted `sed`/regex — do **not** `source` either file.
    An `ANDROID_VERSION_CODE` / `ANDROID_VERSION_NAME` already present in the
    environment (or passed on the `make` command line, which the `Makefile`
    exports) still wins, for throwaway local builds.

    **The override test must be `[ -n "${VAR:-}" ]`, not "is the variable
    set".** The `Makefile` (lines 109–110) exports both names
    unconditionally, and GNU make exports an *undefined* variable as an **empty
    string** (verified). So every plain `make android-aab` reaches the script
    with both variables set-but-empty; an is-set test would read that as an
    explicit override and defeat the file parsing entirely. The script's
    existing `[ -n "${ANDROID_VERSION_CODE:-}" ]` idiom is already correct —
    keep it.
31. `CMakeLists.txt` must **not** carry hand-edited version literals. The
    current lines

    ```cmake
    set(ANDROID_VERSION_CODE "1" CACHE STRING "Android manifest versionCode")
    set(ANDROID_VERSION_NAME "1.0.0-alpha.2" CACHE STRING "Android manifest versionName")
    ```

    must be replaced by a plain (non-cache) read of the environment, so the
    values always come from whoever configured the build:

    ```cmake
    set(ANDROID_VERSION_CODE "$ENV{ANDROID_VERSION_CODE}")
    set(ANDROID_VERSION_NAME "$ENV{ANDROID_VERSION_NAME}")
    ```

    Removing `CACHE` is load-bearing, not cosmetic — see §6.5.

    **Scope of the fix:** CMake reads `$ENV{}` at *configure* time only.
    `build-android.sh` always re-runs `cmake -S . -B …` before
    `cmake --build`, so the values are re-read on every scripted build and the
    stale-cache failure mode is genuinely gone **for the release path**. A bare
    `cmake --build` or a Qt Creator build that skips re-configuring still reuses
    whatever was configured last — acceptable, since those paths deliberately
    produce a non-release version (requirement 32), but it must be stated rather
    than implied.
32. When those environment variables are **absent** (a Qt Creator or plain
    `cmake` developer build), `CMakeLists.txt` must not fail. It must skip
    setting the `QT_ANDROID_VERSION_*` target properties, letting Qt apply its
    defaults (versionCode 1, versionName "1.0"), and emit a
    `message(STATUS)` saying that release packages are built through
    `build-android.sh`. A developer build must never silently produce a
    release-looking version.
33. `build-android.sh` must echo the resolved values, and where each came from,
    at the start **and** at the end of the build next to the artifact path:

    ```
    ==> versionCode : 3              (android/version.txt)
    ==> versionName : 1.0.0-alpha.3  (bridges/Cargo.toml)
    ```

34. `make android-aab` must need **no** version arguments. The documented
    command line in `Makefile`, `CMakeLists.txt` comments and the docs must drop
    `ANDROID_VERSION_CODE=<n> ANDROID_VERSION_NAME=<v>`, and the release
    procedure becomes: edit `android/version.txt` (and `bridges/Cargo.toml` if
    the version changed), then `make android-aab`.

### G. Stop building a debug bundle during release builds

35. A release AAB build must not build, package or sign the debug variant.
    androiddeployqt appends the bare `bundle` Gradle task (not `bundleRelease`),
    which pulls in the whole debug variant — measured at **43 `:*Debug*`
    tasks** including `packageDebugBundle`, `signDebugBundle` and `bundleDebug`,
    whose output is written to `build/outputs/bundle/debug/` and discarded.
36. The fix must be **conditional**: `make android-apk-debug` (`--debug`)
    depends on the debug variant existing. Disabling it must be keyed off a
    Gradle property that `build-android.sh` passes only for release builds,
    e.g.

    ```gradle
    androidComponents {
        beforeVariants(selector().withBuildType("debug")) { variant ->
            variant.enable = !project.hasProperty("simsapaReleaseOnly")
        }
    }
    ```

    **36a — the property must be delivered through the environment, not a `-P`
    argument.** There is no existing Gradle-argument pass-through in
    `build-android.sh`: it runs `cmake -S . -B …` and then
    `cmake --build … --target aab`, and Gradle is invoked by androiddeployqt,
    which accepts no `-P` forwarding from us. Use Gradle's own environment
    mapping instead — `ORG_GRADLE_PROJECT_<name>` becomes a project property:

    ```sh
    # release builds only
    export ORG_GRADLE_PROJECT_simsapaReleaseOnly=true
    ```

    `project.hasProperty("simsapaReleaseOnly")` then works unchanged.
    **Caveat:** `hasProperty` is true for *any* value, including `false` and the
    empty string, so for `--debug` the variable must be left **unset** — never
    set to `false`.

37. Acceptance: a release AAB build log contains no `:*Debug*` packaging tasks,
    the release AAB is still produced and signed, and
    `make android-apk-debug` still succeeds.

### H. Android Gradle Plugin

38. **Do not upgrade AGP as part of this change.** The analysis in §7.4 shows
    the upgrade is not required for targetSdk 36 and that the safe ceiling with
    Qt 6.9.3's bundled Gradle 8.12 wrapper is narrow. Keep AGP **8.6.0** and
    the JDK pin (`MAX_JDK_MAJOR=21`) exactly as they are.
39. Silence the cosmetic compileSdk warning by adding
    `android.suppressUnsupportedCompileSdk=36` to `android/gradle.properties`,
    with a comment pointing at the AGP analysis.

    **This is not yet done, despite `AGENTS.md:509` stating that it is.** The
    file currently ends at `android.useAndroidX=true`, so every build prints the
    warning today. Fixing the file is only half the requirement — the `AGENTS.md`
    sentence must be reconciled with reality at the same time (requirement 47),
    otherwise the doc goes on describing a state that took a release to reach.

    Confirmed safe: androiddeployqt **appends** its generated keys
    (`androidCompileSdkVersion`, `qtTargetAbiList`, …) to the copied
    `gradle.properties` rather than overwriting it — checked against
    `build/android-multiabi/android-build/gradle.properties`, which still
    carries our `org.gradle.parallel` and `android.useAndroidX` lines. A
    project-level property therefore survives into the build.
40. Record the AGP upgrade as a **future** task, coupled to the Qt upgrade (see
    requirement 45), including the Gradle-wrapper and JDK constraints from
    §7.4.

### I. Unresolved QML import warnings

41. Determine whether the androiddeployqt warning
    `QML import could not be resolved: com.profoundlabs.simsapa` is benign.
    Method: compare `assets/android_rcc_bundle/qml/` in the built package
    against the QML the app actually imports, and confirm the app's own QML
    module is compiled into the binary (it is registered through
    `cxx_qt_import_qml_module`, so it has no on-disk plugin directory for the
    scanner to find).
42. Classify each remaining warning (`QtWebEngine`,
    `QtWayland.Compositor`, `QtQuick.Controls.{Windows,macOS,iOS}`,
    `QtQuick3D.MaterialEditor`) as harmless-by-construction, with the reason.
43. Record the conclusion in the docs so the warnings can be ignored
    confidently in future build logs.

### J. `useLegacyPackaging`

44. Re-check `packagingOptions.jniLibs.useLegacyPackaging true` against
    targetSdk 36 and decide explicitly whether to keep it. The decision must be
    backed by measurements taken **both ways**: APK/AAB size, on-device install
    footprint, `zipalign -c -P 16` result, and the ELF `p_align` of the app and
    Qt libraries. Keep the current setting unless the measurements favour
    changing it; record the numbers either way.

### K. Documentation

45. Write `docs/android-qt-upgrade-considerations.md`: why we will eventually
    move off Qt 6.9.3, what it buys, and the pitfalls to guard against —
    including the Qt 6.10.1 problems already recorded in
    `docs/qt-6.10.1-appimage-issues.md` (libtiff SONAME, WebEngine + FUSE
    AppImage crash) and the Android-specific items in §7.5.
46. Write (or extend an existing doc with) the Android edge-to-edge / safe-area
    design: that Qt's `ApplicationWindow` padding supplies the safe area, that
    the app's setting is only *extra* clearance on top of it (and why its
    default had to become 0), the cases Qt does not pad, and how to test it.
    Two rules belong in it explicitly: **never assign `topPadding` / `padding`
    on an `ApplicationWindow` root** (it overwrites Qt's binding silently — §6.3),
    and **`Popup`-family items are not padded**, with `DrawerMenu.qml` as the
    worked example (requirement 15a).
47. Update `AGENTS.md`'s Android section (`CLAUDE.md` is a symlink to it) and
    `docs/android-multi-abi-and-chromeos.md` for the new targetSdk, the
    version workflow (versionCode in `android/version.txt`, versionName from
    `bridges/Cargo.toml`, both exported by `build-android.sh`), and the
    release-only debug-variant switch. The `make
    android-aab ANDROID_VERSION_CODE=<n>` examples in `Makefile`, `CMakeLists.txt`
    and the docs must be updated — that command line is no longer the normal
    path.
48. ~~Delete `tasks/android-packaging-follow-ups.md`~~ — **already gone.** The
    file does not exist and was never committed; its content was folded into
    this PRD during drafting. Nothing to do beyond confirming no doc references
    it.

### L. Closed decisions (no work, recorded for completeness)

49. **Keep `armeabi-v7a`.** We have users on 32-bit ARM phones. The cost is
    ~1/3 of the multi-ABI build time and an extra ~57 MB slice in the bundle
    (which no user downloads — Play delivers one ABI per device). Delivered
    per-ABI downloads measured at arm64-v8a 64.9 MB, x86_64 66.3 MB,
    armeabi-v7a 58.7 MB, all far under Play's 200 MB limit.
50. **Do not add the 32-bit `x86` ABI.** Not used; Chromebook ARCVM is 64-bit;
    it would need the `i686-linux-android` Rust target, which is not installed.
    `build-android.sh` deliberately avoids `QT_ANDROID_BUILD_ALL_ABIS` so the
    installed `android_x86` Qt kit is never picked up by accident.
51. **Do not remove `package=` from `AndroidManifest.xml`**, despite Gradle
    recommending it on every build: it is androiddeployqt's actual source of
    truth for the application id (`extractPackageName()` rejects the literal
    token `androidPackageName` from `build.gradle`'s `namespace` line and falls
    back to this attribute), and changing it would change the application id of
    a published app. The reasoning is already a comment in the manifest.
52. **Bundle size needs no action.** A 284 MB AAB is fine; per-device delivery
    is 59–66 MB compressed.

---

## 5. Non-Goals (out of scope)

- **Upgrading Qt.** This release stays on **Qt 6.9.3**, which is known to work
  for our Android, AppImage and desktop builds. Qt 6.10.1 caused concrete
  problems recorded in `docs/qt-6.10.1-appimage-issues.md`. The upgrade is a
  separate, later task; this PRD only produces the analysis document for it.
- **Upgrading the Android Gradle Plugin, the Gradle wrapper, the NDK, or the
  JDK pin.** All stay put (requirement 38).
- **A visual redesign.** We are not adopting a "true" edge-to-edge look where
  content deliberately paints behind translucent system bars. The target is
  *visually near-identical to today*.
- **Removing Qt's deprecated `setStatusBarColor` / `setNavigationBarColor`
  calls.** They are in Qt's Java, not ours.
- **Emulator-based verification.** Testing is on a real Android phone
  (see §9).
- **Dropping `armeabi-v7a`** (requirement 50).
- **Any change to the application id or the QML module URI.** They are
  unrelated identifiers and both stay as they are.

---

## 6. Design Considerations

### 6.1 The current top-margin mechanism, and why it was not enough

**How it is applied today.** The value is a plain number pushed into each
window's root layout as an anchor margin, *inside* the window's content:

```qml
// SuttaSearchWindow.qml:2481-2483
ColumnLayout {
    anchors.fill: parent
    anchors.topMargin: root.top_bar_margin
```

and handed down to the child windows as `required property int top_bar_margin`,
which apply it the same way (e.g. `AboutDialog.qml:80`). The number is
`is_mobile ? SuttaBridge.get_mobile_top_bar_margin() : 0`, resolved as
`status_bar_height` (JNI resource lookup, px ÷ density) or the user's custom
value, with a hard-coded `24` fallback while `APP_DATA` is still initializing.
No window sets `topPadding` or `padding` on its `ApplicationWindow` root.

**Why it was needed at all.** The setting was added on **2025-12-11**. At that
point the Android build used **Qt 6.8.3** — which has neither `SafeArea` nor
`ApplicationWindow`'s automatic safe-area padding (both arrived in Qt 6.9). Qt
therefore applied *no* inset whatsoever, while on Android 15 devices the
platform had *already* made the app edge-to-edge (targetSdk 35 with no
`windowOptOutEdgeToEdgeEnforcement`), so window content genuinely started at
y = 0, under the status bar. The manual margin was the entire compensation.

Worth noting: Qt does not letterbox the window either. `setSystemUiVisibility()`
early-returns when its two flags are unchanged (`QtDisplayManager.java:135`),
and both default to `false`, so the `setDecorFitsSystemWindows(true)` branch is
**never executed at startup**. Whatever the platform does is what the app gets.

**Why it was insufficient for some users.** `status_bar_height` is the height of
the *status bar*, not the size of the *safe area*. Qt's own inset computation
asks the platform for

```java
// QtWindow.getSafeInsets()
int types = WindowInsets.Type.displayCutout() | WindowInsets.Type.systemBars();
return insets.getInsets(types);
```

i.e. the **union of the system bars and the display cutout**. On a punch-hole or
notch device the cutout extends past the status bar, so `status_bar_height`
under-reports exactly the amount by which the OS elements overlapped the search
bar. That is why a *fixed* default of 24 dp existed as a floor, why the value
was made user-editable, and why the complaints came from particular devices
rather than all of them. The mechanism was not wrong — it was fed a number that
is systematically too small on the devices that had the problem.

**What changed underneath it.** Since **2025-12-18** the Android build uses
**Qt 6.9.3**, where `ApplicationWindow` binds its own padding to the window's
safe area (§6.3). The manual margin sits *inside* that padding, so on an
Android 15/16 device the current build should already be applying **both** — the
real inset from Qt plus `status_bar_height` again from us.

> **Checked on device (2026-07-27):** the gap above the search bar in the
> current build on the Android 16 test phone is **roughly twice the status-bar
> height**. That confirms the model: Qt is applying the real inset *and* we are
> applying `status_bar_height` again on top. It is why the fix is a default
> change rather than a new mechanism (§6.3), and why the doubling is a bug in
> the **shipped** build, independent of targetSdk 36 (requirement 20).

**Consequence for the setting's semantics.** The historical failure was the
value being **too small**, never too large. Qt's inset is the correct quantity
(bars ∪ cutout, live, per window), so the only thing left for a user setting is
the ability to add *more* clearance on top of it. That is exactly what the
existing anchor margin does — it just defaults to a value that duplicates the
inset. Hence requirement 8: keep the mechanism, change the default to 0, and
rename it to say "extra".

It also means the setting must **not** be able to reduce the padding below the
platform's inset. Under enforced edge-to-edge a smaller number is no longer
merely cosmetic: it puts the search bar back under the OS buttons, the very bug
this feature was created to fix. A non-negative "extra" value cannot express
that, which is a property worth keeping deliberately.

### 6.2 Is the top bar margin equivalent to `SafeArea.margins.top`?

**Short answer: usually the same number on a plain phone, but not the same
quantity — and it stops matching exactly where it matters.**

The two values are in the same units, which is the part worth confirming first:

- `get_mobile_top_bar_margin()` reads the `android:dimen/status_bar_height`
  resource in pixels and divides by `DisplayMetrics.density`
  (`cpp/utils.cpp:30`), yielding dp.
- Qt's Java side reports insets in **physical pixels**
  (`QtWindow.getSafeInsets()` → `QAndroidPlatformWindow::safeAreaMarginsChanged`,
  which stores them unmodified), and `QWindow::safeAreaMargins()` converts with
  `QHighDpi::fromNativePixels()`. QML `SafeArea.margins.*` are therefore in the
  same logical units QML lays out in.

So on a phone with no cutout, no action bar and gesture navigation, the top
values coincide. They diverge in four ways:

| | `mobile_top_bar_margin` | `SafeArea.margins.top` |
|---|---|---|
| Definition | Height of the status bar, from a **static dimension resource** | `max(status bar, display cutout)` — `QtWindow.getSafeInsets()` asks for `WindowInsets.Type.displayCutout() \| systemBars()` |
| Notch / punch-hole devices | Under-reports: the cutout can exceed the status bar height | Correct |
| Action bar | Not considered | Explicitly subtracted when the action bar is hidden (Qt works around insets that include it) |
| Landscape | Same value as portrait — the resource does not change | Reflects the actual inset for the current orientation |
| Freshness | Read once at load and on settings change | Live: re-reported on inset change, layout change and pre-draw |
| Relative to | The screen | **The item it is attached to** — margins already consumed by an ancestor are subtracted |

That last row is the subtle one: `SafeArea` is an attached property whose values
are *relative to the item*, so applying it at the root and again in a child does
not double-count.

And the whole comparison only covers the top. The other three edges have no
equivalent today at all — which is exactly what enforced edge-to-edge exposes:
the bottom gesture bar, and in landscape the cutout side.

**Conclusion:** treat `status_bar_height` as a superseded approximation of the
top inset. Requirement 8 replaces it with the safe-area value; §7.2 keeps the
manual override as insurance against Qt reporting it wrongly.

### 6.3 Why one *extra* top knob, and not a four-edge setting

An earlier draft of this PRD proposed replacing the top margin with a four-edge
`MobileSafeAreaMargins` setting, each edge resolvable to a system inset or a
custom value, plumbed through the bridge. **The device measurement retires that
design.** The doubled top gap proves Qt is already delivering the system inset
on all four edges, correctly, so a settings-based reimplementation of it would
be re-deriving a value the framework has already applied.

**What Qt provides.** Since Qt 6.9, `ApplicationWindow` installs property
bindings from its content's safe area onto its own padding:

```cpp
// qquickapplicationwindow.cpp:801-805
installPropertyBinding(this, "leftPadding"_L1,   controlSafeArea, "margins.left"_L1);
installPropertyBinding(this, "topPadding"_L1,    controlSafeArea, "margins.top"_L1);
installPropertyBinding(this, "rightPadding"_L1,  controlSafeArea, "margins.right"_L1);
installPropertyBinding(this, "bottomPadding"_L1, controlSafeArea, "margins.bottom"_L1);
```

documented as *"ApplicationWindow will automatically add padding to the
contentItem for any safe area margins reported by the window … while the
background item covers the entire window."* Declared children are reparented into
that contentItem, so every `anchors.fill: parent` layout in this app is inset
automatically — **live, per window, and cutout-aware**. Three properties a
settings value cannot have:

| | Qt's padding | A settings value |
|---|---|---|
| Per window | yes — each of the ten `ApplicationWindow`s gets its own | one global number for all |
| Updates on rotation / inset change | yes, by binding | only when re-read |
| Cutout-aware | yes (`displayCutout() \| systemBars()`) | only what we compute |

**The binding is overwritable — this is a standing rule, not a detail.** The
lambda ends in `binding.installOn(targetProperty)`, so an explicit
`topPadding` / `padding` assignment on *any* `ApplicationWindow` root silently
replaces Qt's safe-area binding and the inset disappears for that window with no
warning. Verified today: no window sets either (every `padding:` hit in
`assets/qml/` is on an inner control). This must be written into the new doc
(requirement 46) as the first thing to check if an inset ever goes missing.

**What remains for the setting.** Exactly one thing: extra clearance on a device
where the platform's own inset still leaves the toolbar covered. That is a
scalar the user tunes, and it is already how the existing margin behaves — an
anchor margin *inside* the padding. So the correct change is not a new
mechanism; it is **fixing the default from `status_bar_height` to 0** and
renaming the setting to say what it now means.

**Why the numbers survive the change.** The old value was already being added on
top of Qt's padding on any Qt 6.9.3 build — that is what the doubling *is*. So
`CustomValue(v) → v` leaves a customising user's screen pixel-identical, and
`SystemValue → 0` removes the duplicate for everyone else (requirement 9). No
reinterpretation, no reset, no lost preferences.

**Why not add bottom/left/right knobs anyway?** Because there is no evidence any
other edge is under-reported, each knob is a way for a user to break their own
layout, and the pattern is trivially repeatable if the device pass turns one up
(requirement 14). Note also the asymmetry in risk: extra *top* margin can only
push content further from the bars, whereas the retired four-edge design had a
`CustomValue(0)` that would have pushed content *under* them — the very bug the
feature exists to prevent.

**What Qt does *not* pad, and must still be handled by hand** (requirement 15):

- **`header`, `footer` and `menuBar`** are siblings of the contentItem and are
  deliberately left unpadded (`qquickapplicationwindow.cpp:167-171` instead adds
  their heights as *additional* margins to the content). In this app
  `SuttaSearchWindow`'s `menuBar` is `visible: root.is_desktop`, and the two
  `footer:` uses are inside `Dialog`s rather than windows — so nothing is
  currently exposed, but a new mobile-visible `header`/`footer` would need its
  own inset.
- **Inline `Dialog` / `Popup` items** (present in ~14 files) are positioned in
  the window overlay, not the contentItem, so they receive no padding. Centered
  dialogs are unaffected; a tall or top-anchored one can reach under a bar.
  **`DrawerMenu.qml` is the confirmed case** — a full-height `Drawer` in the
  overlay whose first child is a top-anchored `Label`, and which is the mobile
  main menu (requirement 15a).
- **`Flickable` / `ListView` content.** Padding insets the viewport, which is
  what we want; Qt's own snippet documents a `contentY` correction for a
  changing `topMargin` (QTBUG-131478) if a Flickable's margins are ever bound to
  safe-area values directly.
- **The WebEngine/WebView panels** are ordinary children, so they are inset with
  the contentItem — but their *internal* scrolling content needs a check that no
  fixed-position HTML element ends up under a bar (requirement 16).

### 6.4 Windows that consume the margin today

Ten components carry `property int top_bar_margin: is_mobile ? 24 : 0` (or the
`required property int` receiving end) — ~88 references across `assets/qml/`:

`SuttaSearchWindow`, `AppSettingsWindow`, `LibraryWindow`, `DictionariesWindow`,
`SuttaLanguagesWindow`, `TopicIndexWindow`, `ReferenceSearchWindow`,
`ChantingPracticeWindow`, `ChantingPracticeReviewWindow`,
`DictionaryImportDialog`.

**Every one of them has an `ApplicationWindow` root** — including the ones named
"…Dialog" (`AboutDialog`, `SystemPromptsDialog`, `ModelsDialog`,
`AnkiExportDialog`, `DatabaseValidationDialog`, `DhammaTextSourcesDialog`,
`UpdateNotificationDialog`, `DictionaryImportDialog` are all
`ApplicationWindow { flags: Qt.Dialog }`, i.e. real top-level windows). That is
what makes Qt's automatic padding apply uniformly to all of them. The
parent-to-child `top_bar_margin` plumbing stays as it is: with the new
semantics it carries a **global user preference** (extra clearance), not a
per-window inset, so passing one value down is correct (requirement 13).

The one exception is `assets/qml/MobileTopMarginDialog.qml`, whose root is a
`Dialog` — and which is referenced by no other QML file and absent from
`bridges/build.rs`'s `qml_files`. Dead code, to be deleted (requirement 18).

Two details in that set that the rename must not trip over:

- **`AppSettingsWindow.qml` owns the property but never reads the bridge.** Its
  `top_bar_margin` (line 28) is assigned by whoever creates the window; unlike
  the nine windows that call `get_mobile_top_bar_margin()`, it only *consumes*
  the value (lines 189, 356, 930). The rename must cover those three sites even
  though no bridge call changes there.
- **`TopicIndexWindow.qml:25` defaults to `is_mobile ? 24 : 5`** — a desktop `5`
  that has nothing to do with insets. **It is already dead code:** line 100's
  `Component.onCompleted` overwrites the property with
  `root.is_mobile ? SuttaBridge.get_mobile_top_bar_margin() : 0`, so on desktop
  the `5` survives only the frames before completion and every desktop user
  already sees `0`. Set the declaration to `0` and do **not** re-home the `5`
  anywhere — moving it into the layout would *introduce* 5px of desktop spacing
  that does not exist today.

  The same is true of every owner's `is_mobile ? 24 : 0` default: it is a
  transient initializer, overwritten on `Component.onCompleted`. Task 2.3's
  change to `0` therefore only affects the pre-completion frames — which is
  precisely where the old default caused a visible jump on mobile.

### 6.5 Where the version values live

**One flow, no duplicates:**

```
android/version.txt        3              ─┐
                                           ├─→ build-android.sh
bridges/Cargo.toml   version = "1.0.0-…"  ─┘        │
                                                    │ exports
                                                    ▼
                                     ANDROID_VERSION_CODE / _NAME (env)
                                                    │
                                                    ▼
                              CMakeLists.txt  →  QT_ANDROID_VERSION_*
                                                    │
                                                    ▼
                       androiddeployqt → AndroidManifest.xml placeholders
```

**Why `CMakeLists.txt` must not hold the values.** It currently does — as
`CACHE STRING` defaults that the build script then overrides with `-D`
arguments. That is the worst of both: a hand-editable literal that is *usually
ignored*, and a cache entry that is *sometimes* not. Two concrete failure modes:

- **The cache never updates.** A cache variable is written once per build
  directory. With an existing `build/android-multiabi/` tree, editing the
  literal changes nothing, and a build invoked without `-D` silently reuses
  whatever was configured the first time. That is precisely the "convenient but
  easy to misread" trap noted when the multi-ABI build shipped.
- **Two sources of truth for the name.** `CMakeLists.txt` and
  `bridges/Cargo.toml` both spell out `1.0.0-alpha.2` by hand today, with
  nothing keeping them equal.

Making `CMakeLists.txt` a **pure consumer** of the environment removes both. The
values are read once, by the script, from the files a human edits.

**Why the versionCode gets its own file rather than living in `Cargo.toml`.**
It cannot be derived from the version string: re-uploading a fixed build of the
same tagged version needs a higher code but keeps the name. It is also the one
value with an external constraint (Play's strictly-increasing rule), so a file
whose entire content is that number — and whose git history is the upload
history — is the clearest place for it.

**Parse, never `source`.** Sourcing a `.toml` as shell would execute arbitrary
content and leak `export`s into Gradle and CMake; `android/version.txt` is
deliberately a bare number, not `KEY=value`, so there is nothing to source in
the first place.

**Developer builds.** Qt Creator and plain `cmake` invocations do not go through
`build-android.sh`, so the environment variables are absent. `CMakeLists.txt`
then leaves `QT_ANDROID_VERSION_*` unset and Qt's defaults (versionCode 1,
versionName "1.0") apply — an obviously-not-a-release marking, which is the
right outcome for a dev APK. Release packages come only from the script.

### 6.6 Visual acceptance

"Near-identical to today" means: on the test phone, screenshots before and
after the change differ only by inset-driven spacing, and specifically —

- the search bar and toolbar sit fully below the status bar;
- no button, tab or list row is under the gesture bar;
- in landscape nothing is under a cutout or rounded corner;
- opening the soft keyboard does not hide the focused input (this interacts
  with the existing `MobileKeyboardHelper` behaviour — see
  `docs/android-soft-keyboard.md`);
- WebEngine/WebView reader panels are not clipped at the top or bottom.

### 6.7 Where `AppSettings` is deserialized (three sites, not one)

An earlier draft assumed `db::get_app_settings()` was the single
`serde_json::from_str::<AppSettings>` site and that a migration hook could be
dropped in next to it. It is not. Verified:

| Site | Role |
|---|---|
| `backend/src/db/appdata.rs:443` — `AppdataDbHandle::get_app_settings()` | **The in-app path.** Fills `app_settings_cache`; this is what the running GUI reads |
| `backend/src/db/mod.rs:389` — free `get_app_settings()` | Standalone read before `QApplication` exists (`gui.cpp`'s `render_loop_basic` pre-flight) |
| `backend/src/app_data.rs:3237` | Settings **import** from `import-me/app_settings.json` |

A hook placed only at `db/mod.rs:389` would migrate the pre-flight copy and
leave the running app's cache on the default — i.e. the migration would appear
to do nothing.

`appdata.rs:443` already performs exactly this kind of post-deserialize fixup
(`settings.merge_default_system_prompts()`), which is the precedent for *where*
such logic goes. But the correct answer here is one level lower: put the
carry-over in `AppSettings`' own deserialization (`#[serde(from = "…")]` over a
private wire struct, or a manual `impl Deserialize`), so it cannot be bypassed
and a fourth reader added later inherits it for free.

Two consequences for the implementation:

- The legacy capture field must be `#[serde(skip_serializing)]`, or the old
  `mobile_top_bar_margin` key is written straight back out on the next save.
- The old `MobileTopBarMargin` enum cannot simply be deleted while a capture
  field is typed as it. Keep it `pub(crate)` inside the wire struct, or capture
  the legacy key as `Option<serde_json::Value>` and match on it.
- "Migrate only when the new key is absent" is not expressible against a plain
  `u32` field under the container-level `#[serde(default)]`: absent and explicit
  `0` are indistinguishable. Either capture the new key as `Option<u32>` in the
  wire struct, or adopt the simpler equivalent rule — **migrate when the new
  value is `0`** — which is safe precisely because `SystemValue → 0` is the
  intended result anyway.

---

## 7. Technical Considerations (findings and diagnosis)

### 7.1 What targeting API 36 actually switches on

From Android's "Behavior changes: apps targeting Android 16 or higher":

- **Edge-to-edge**: `windowOptOutEdgeToEdgeEnforcement` is deprecated and
  disabled at targetSdk 36. (At targetSdk 36 *running on an Android 15 device*
  it still works — which is why testing must be on Android 16.)
- **Predictive back**: enabled by default; `onBackPressed()` is not called and
  `KEYCODE_BACK` is not dispatched. Temporary opt-out:
  `android:enableOnBackInvokedCallback="false"`.
- **Adaptive layouts ≥ 600dp**: `android:screenOrientation`,
  `android:resizableActivity`, `android:minAspectRatio`,
  `android:maxAspectRatio`, `setRequestedOrientation()` and
  `getRequestedOrientation()` are ignored. Temporary opt-out via
  `android.window.PROPERTY_COMPAT_ALLOW_RESTRICTED_RESIZABILITY`, which stops
  working at API 37.
- Also in the list but **not applicable to this app**: elegant-font API
  deprecation, health/fitness permission granularity, Bluetooth bond intents,
  `MediaStore#getVersion()`, safer-intents opt-in, photo-picker pre-selection.
- **Local network permission** (`NEARBY_WIFI_DEVICES`, opt-in phase now,
  enforcement later) does **not** affect the app's embedded Rocket webserver:
  that is loopback, not local-network access. Worth a re-check at a future
  targetSdk bump.

### 7.2 Qt 6.9.3 and edge-to-edge — what exists and what is uncertain

**What exists.** Qt 6.9 introduced safe-area support and it is present in the
installed 6.9.3 Android kits:

- `~/Qt/6.9.3/android_arm64_v8a/include/QtQuick/6.9.3/QtQuick/private/qquicksafearea_p.h`
- `QtWindow.java` registers an `OnApplyWindowInsetsListener` and calls
  `reportSafeAreaMargins()`, including a pre-draw hook so margins are delivered
  before the first frame.

So the mechanism does not require a Qt upgrade.

**What is uncertain.** There are reports of edge-to-edge behaving differently
on Android 16 than on Android 15 with Qt 6.8/6.9, tracked upstream as
**QTBUG-140193**, with at least one report that safe areas work correctly from
**Qt 6.10.1+**. We have not reproduced or refuted this on 6.9.3.

**Mitigation.** Two things cover it. First, the extra-top-margin setting
(requirement 8) remains the user-side fix if the reported top inset is still too
small on some device — the same role it has played since 2025-12-11, only with a
sane default. Second, requirement 17 asks for an explicit re-check that the
margins keep arriving at targetSdk 36; they demonstrably arrive at targetSdk 35
today, which is what the doubled gap proves (§6.1). The on-device
test in §9 is the gate — if the top inset is systematically wrong on the test
phone, fall back to using `status_bar_height` for the top edge and
safe-area only for the other edges, and record that.

### 7.3 The deprecated window-colour APIs are Qt's, not ours

Verified in the installed Qt sources
(`~/Qt/6.9.3/Src/qtbase/src/android/jar/src/org/qtproject/qt/android/`):

| Play-reported API | Location |
|---|---|
| `Window.getStatusBarColor` | `QtActivityDelegateBase.java:108` (luminance check to pick light/dark status-bar icons) |
| `Window.setStatusBarColor` | `QtDisplayManager.java:191`, `:200` |
| `Window.setNavigationBarColor` | `QtDisplayManager.java:192`, `:204` |

These sit in `QtDisplayManager.setSystemUiVisibility()`, exactly where Play's
report says the calls originate. The app has no way to remove them short of
patching Qt's Java or upgrading Qt. At API 36 the setters are ignored by the
platform, so the practical effect is nil — the report is informational, not a
blocker. Requirement 19 therefore documents rather than "fixes".

Note the app never enables Qt's cutout expansion (`expandedToCutout`) or
fullscreen mode, so the `Color.TRANSPARENT` branch of that code is not the one
being taken today.

### 7.4 AGP upgrade — risk analysis (requirement 38)

**Does targetSdk 36 require a newer AGP? No.** `targetSdkVersion` is a value
written into the manifest; AGP does not gate it. `compileSdk` is the version
AGP validates against, and androiddeployqt already writes
`androidCompileSdkVersion=android-36` (it picks the newest installed platform —
we have android-35 and android-36 installed). AGP 8.6.0 accepts that with a
warning, which is what the current build already prints:

> WARNING: We recommend using a newer Android Gradle plugin to use compileSdk = 36
> This Android Gradle plugin (8.6.0) was tested up to compileSdk = 35.

**What an upgrade would cost.** Three coupled constraints:

1. **Gradle wrapper.** Qt 6.9.3 ships the Gradle wrapper at **8.12**
   (`~/Qt/6.9.3/android_arm64_v8a/src/3rdparty/gradle/gradle/wrapper/gradle-wrapper.properties`).
   AGP 8.10 requires Gradle ≥ 8.11.1 — compatible. AGP 8.11+ requires Gradle
   **8.13**, i.e. diverging from the Qt-provided wrapper as well as from the
   Qt-provided `build.gradle` template.
2. **JDK.** The pin to 17–21 (`MAX_JDK_MAJOR=21` in `build-android.sh`) exists
   because AGP 8.6.0's bundled lint cannot parse a Java 26 version string; the
   failure is a `lintVitalAnalyzeRelease` crash whose entire error message is
   `> 26.0.1`, emitted **after** all three ABIs have compiled and signed. The
   machine's default `java` is 26. Any AGP change must re-verify this pin, and
   the pin should stay regardless.
3. **Deprecated DSL.** Qt's template uses `lintOptions`, `aaptOptions` and
   `packagingOptions` — deprecated across AGP 8.x and **removed in AGP 9.x**.
   Moving to AGP 9 means rewriting a Qt-provided template, which then has to be
   re-merged on every Qt upgrade.

**Conclusion.** The upgrade buys a suppressible warning and no functional
benefit for this release, while touching the Gradle wrapper, the JDK pin and a
Qt-provided template — three things that each independently break the build
late and unhelpfully. Change one variable at a time: take targetSdk 36 now on
AGP 8.6.0, suppress the warning (requirement 39), and revisit AGP together with
the Qt upgrade, when the Qt-provided template changes anyway.

### 7.5 Future Qt upgrade — reasons and pitfalls (input for requirement 45)

**Reasons to upgrade eventually:**

- Removes the deprecated `setStatusBarColor` / `setNavigationBarColor` calls
  from Play's report (expected in a newer Qt).
- Reported safe-area fixes for Android 16 (QTBUG-140193; safe areas reported
  working on 6.10.1+).
- Qt 6.10 officially lists Android 9–16 support and ships 16 KB page support
  out of the box, removing the need for our explicit
  `-Wl,-z,max-page-size=16384` link flag.
- Newer AGP/Gradle templates, which is what unblocks §7.4.
- Staying on a Qt release that still receives fixes.

**Pitfalls to guard against, from our own history:**

- **AppImage / libtiff.** Qt 6.10.1's `libqtiff.so` links `libtiff.so.5` while
  Arch ships `.so.6`; `linuxdeploy` fails. Current workaround: temporarily move
  `libqtiff.so`/`libqtga.so` aside during deployment.
- **AppImage / WebEngine + FUSE.** Qt 6.10.1's WebEngine SIGSEGVs when run from
  a FUSE-mounted AppImage; the workaround is a wrapper that forces
  `--appimage-extract-and-run`, costing 2–5 s of startup and ~600 MB of temp
  space.
- **minSdk floor — we are already below Qt's stated minimum.** The framing that
  the 28 floor *arrives* with Qt 6.10 is wrong: androiddeployqt already writes
  `qtMinSdkVersion=28` into the generated `gradle.properties` on **Qt 6.9.3**,
  and `android/build.gradle`'s `defaultConfig` overrides it back down to
  `minSdkVersion 27`. So the app ships one API level below what the current Qt
  declares it supports, and has done since the 6.9.3 move. Qt 6.10 does not
  introduce the constraint — it removes our ability to keep ignoring it. This
  makes open question 4 (§10) a live question now, not one deferred to the
  upgrade: either we have evidence API 27 works (it demonstrably has, for
  shipped users), or the override should be reconsidered on its own merits.
- **NDK.** Qt 6.9.3 must stay on NDK r26b/r27 — r28 breaks the `cxx` C++ build
  at minSdk 27 (`pthread_cond_clockwait` needs API 30+). A Qt upgrade changes
  the supported NDK, which changes the Rust/`cxx` build surface.
- **cxx-qt.** The bridge layer is version-sensitive; a Qt bump means
  re-verifying every `#[qinvokable]` bridge and the QML module registration.
- **The multi-ABI ExternalProject mechanism**, the per-ABI Qt path guard, the
  cross-ABI plugin-staging bug and its `packagingOptions.jniLibs.excludes`
  workaround all depend on androiddeployqt behaviour and must be re-verified.
- **Desktop, Windows and macOS builds** ride on the same Qt version — an
  Android-motivated upgrade is not an Android-only change.

**Recommended shape of the upgrade task:** upgrade Android first on a branch
with the desktop builds pinned, verify the full manual test plan, then move
desktop/AppImage separately.

### 7.6 x86_64 and armeabi-v7a have never been *run*

The July 2026 multi-ABI work verified that the x86_64 and armeabi-v7a slices
compile, link, package and carry the right ELF machine type — **nothing more**.
Nobody has ever run Simsapa on either.

Plausible runtime risks, still unchecked:

- the pure-Rust audio stack (`cpal` AAudio backend, `flacenc`, `rubato`,
  `symphonia`);
- tantivy index files — the shipped fulltext index is built on x86_64 Linux, so
  x86_64 Android is if anything *safer* than arm64, but 32-bit armv7 has a
  different `usize` and tantivy's maintainers describe 32-bit as little-used;
- JNI paths: `android_saf.rs`, `ndk_context`, `android_helpers.cpp`;
- Qt WebView / Chromium under ARCVM.

Since we are keeping armeabi-v7a for real users (requirement 50), the armv7
slice deserves a real device run at least once. This PRD's test plan is
phone-based (§9); the emulator route remains available if a device is not:

```sh
~/Android/Sdk/cmdline-tools/latest/bin/avdmanager create avd \
    -n simsapa_x86_64 -k "system-images;android-36;google_apis_playstore;x86_64"
~/Android/Sdk/emulator/emulator -avd simsapa_x86_64 &
make android-apk
adb install -r <the signed apk>
```

(Both API 35 and 36 Play-Store x86_64 system images are already installed; no
AVD is defined yet.)

### 7.7 Current build environment (for reference)

| Component | Version |
|---|---|
| Qt (Android kits: arm64_v8a, armv7, x86, x86_64) | 6.9.3 |
| Qt (desktop) | 6.8.3, 6.9.3, 6.10.1 (`gcc_64` only) |
| Android SDK platforms | android-35, android-36 |
| Build-tools | 36.0.0 |
| NDK | 27.3.13750724 |
| AGP | 8.6.0 |
| Gradle wrapper (from Qt) | 8.12 |
| JDK available | 21 (pinned for builds), 26 (system default — must not be used) |
| minSdk / targetSdk | 27 / 35 → **36** (note: Qt 6.9.3 itself declares `qtMinSdkVersion=28`; `build.gradle` overrides down to 27 — see §7.5) |
| ABIs | arm64-v8a; x86_64; armeabi-v7a |

### 7.8 Files likely to change

- `android/build.gradle` — targetSdk, release-only debug-variant disable
- `android/gradle.properties` — `android.suppressUnsupportedCompileSdk=36`
- `android/AndroidManifest.xml` — only if the predictive-back opt-out is needed
- `android/version.txt` — new; holds the versionCode (seed: 3)
- `build-android.sh` — parse the two files, export the env vars, echo them, drop
  the version-arguments requirement, release-only Gradle property
- `CMakeLists.txt` — replace the cached version literals with a plain read of
  the environment, and skip `QT_ANDROID_VERSION_*` when unset
- `Makefile` — drop `ANDROID_VERSION_CODE=…` from the documented command line
- `backend/src/app_settings.rs` / `bridges/src/sutta_bridge.rs` — the single
  `mobile_extra_top_margin` setting, its migration, and the renamed bridge API
- `backend/src/app_data.rs` — the collapsed setter; the import path at 3237 is
  covered automatically if the migration lives in `Deserialize` (§6.7)
- `backend/src/db/appdata.rs` — no change needed if the migration lives in
  `Deserialize`; listed because line 443 is the in-app deserialization site and
  is where a call-site hook would otherwise have to go
- `assets/qml/DrawerMenu.qml` — `topPadding: SafeArea.margins.top` on the
  `Drawer` root (requirement 15a)
- `cpp/utils.cpp` — `get_status_bar_height()` leaves the margin path; keep only
  if Settings still displays it, otherwise delete
- `assets/qml/*.qml` (the ten windows listed in §6.4) — rename the property to
  match the new meaning; the plumbing itself is unchanged
- `assets/qml/MobileTopMarginDialog.qml` — delete (dead code)
- `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` — stubs for any new
  bridge methods
- `docs/android-multi-abi-and-chromeos.md`, new
  `docs/android-qt-upgrade-considerations.md`, `AGENTS.md` (`CLAUDE.md` is a
  symlink to it)
- delete `tasks/android-packaging-follow-ups.md`

---

## 8. Success Metrics

1. Play accepts an upload of the new AAB with **no** target-API compliance
   warning.
2. Zero visual regressions on the test phone against the pre-change build: no
   element obscured by a system bar in portrait or landscape.
3. The gap above the search bar equals **one** safe-area inset, not two — on a
   fresh install and on an upgraded one that never customised the margin.
4. A user who had set a custom top margin keeps the identical layout after the
   update: their value carries over unchanged as extra clearance.
5. Back navigation verified equivalent to the current build.
6. A release AAB build log contains **zero** `:*Debug*` packaging tasks, and
   `make android-apk-debug` still succeeds.
7. `make android-aab` with no version arguments produces a bundle whose
   manifest carries the versionCode from `android/version.txt` and the
   versionName from `bridges/Cargo.toml` — verified with `aapt2 dump badging`.
   Editing either file and rebuilding **in an existing build directory** through
   `build-android.sh` changes the manifest (the current `CACHE` behaviour would
   not). The script re-runs `cmake -S . -B` every time, which is what makes the
   `$ENV{}` read take effect; a bare `cmake --build` is out of scope.
8. Every item in §4 is either implemented or closed with a recorded decision,
   and nothing from the superseded `tasks/android-packaging-follow-ups.md` is
   lost.
9. `DrawerMenu.qml`'s "Menu" label clears the status bar / cutout on the test
   phone with the drawer open, in portrait and landscape.

---

## 9. Manual Test Plan (human-run, on a real Android phone)

Per the project rule, agents do not drive the GUI; this is a human run. Testing
is on a physical Android phone — if the visuals are right there, the other
platforms are expected to follow.

**Device requirement:** the phone must be running **Android 16** — the usual
test phone already does. Edge-to-edge enforcement at targetSdk 36 does not apply
on Android 15 and below, so an Android 15 phone could not validate this change.

**Build:** `make android-apk` (versions come from `android/version.txt` and
`bridges/Cargo.toml`)
and sideload it (copy to the phone and tap to install). Do **not** deploy from
Qt Creator: that triggers the spurious "This app isn't 16 KB compatible" dialog
(gated on the install path, not the package contents) and its kits are
single-ABI.

Checklist:

1. **Cold start / first run** — including the first-run asset download if
   testing on a clean install.
2. **Portrait layout** — status bar area: search bar and toolbar fully visible,
   with the gap now a *single* inset; gesture-bar area: bottom controls fully
   tappable. Check the secondary windows too (Settings, Library, Dictionaries,
   Chanting Practice, About) — each is its own `ApplicationWindow` and is padded
   independently by Qt.
3. **Landscape layout** — rotate in each main window; check cutout/rounded
   corner sides.
4. **Rotation while a dialog is open** — margins update without restart.
5. **Soft keyboard** — focus a search field and a multi-line field; the focused
   input stays visible; keyboard raises on the first tap
   (`docs/android-soft-keyboard.md`).
6. **Settings → Extra Top Margin** — with the default 0, confirm the top gap is
   a single inset; raise it and confirm the extra space appears immediately and
   survives a restart. Confirm the displayed system safe-area value updates when
   the phone is rotated with Settings open.
7. **Upgrade path** — install the new build **over** an existing install that
   has a custom top margin set; confirm the layout is unchanged (the value
   carries over as extra clearance). Then over one that never customised it;
   confirm the doubled gap is gone.
8. **Back navigation** — system back gesture and any hardware back: from a
   dialog, from a secondary window, from the main window. Record the behaviour
   (this decides requirement 23).
9. **Sutta reader (WebView)** — open a sutta, scroll to top and bottom, check
   nothing is clipped; use the find bar; switch display layouts.
10. **Fulltext search (tantivy)** and **dictionary lookup (SQLite/DPD)** —
    confirm results and snippets.
11. **Chanting practice** — record and play back (exercises the pure-Rust audio
    stack).
12. **File save via SAF** — save an export to a user-chosen folder.
13. **Large-screen behaviour** — if a tablet or Chromebook is available, resize
    the window and rotate; confirm nothing depends on a fixed orientation.
14. **armeabi-v7a** — if a 32-bit ARM device is available, repeat items 1, 9,
    10 and 11 on it.

---

## 10. Open Questions

**Answered while drafting:** the last versionCode uploaded to Play is **2**, so
`android/version.txt` is seeded with **3** (requirement 29); and the usual test
phone runs
**Android 16**, so on-device verification of edge-to-edge is possible without an
emulator.

1. **Does Qt 6.9.3 report correct safe-area insets on Android 16?** This is the
   one genuine unknown (QTBUG-140193). Resolved by the first on-device test; if
   it does not, §7.2's fallback applies and the custom override becomes the
   documented workaround.
2. **Does any window need an inset Qt's automatic `ApplicationWindow` padding
   does not give it?** §6.3 lists the known gaps (header/footer/menuBar, inline
   popups, WebView-internal content); the on-device pass decides whether any of
   them actually bites, and whether any edge other than the top ever needs its
   own knob (requirement 14).
3. **Does predictive back regress?** Decides requirement 23; test first.
4. **Should `minSdkVersion` stay at 27?** No reason to change it now, but it
   becomes a forced question at the Qt 6.10 upgrade, which requires 28.
