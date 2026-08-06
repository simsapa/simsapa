# Android: Qt upgrade considerations

Simsapa is pinned to **Qt 6.9.3**. Several pieces of work were deliberately
*not* done during the July 2026 API 36 compliance push because they are blocked
on, or only sensible alongside, a Qt upgrade. This document records that
deferred work, the reasons to upgrade, and the pitfalls to guard against — so
the next person does not have to re-derive any of it.

Companion documents:

- [android-multi-abi-and-chromeos.md](./android-multi-abi-and-chromeos.md) — the
  multi-ABI build mechanism, the manifest/permissions rules, the AGP pin.
- [android-edge-to-edge-and-safe-areas.md](./android-edge-to-edge-and-safe-areas.md)
  — how safe-area padding works and what Qt does *not* pad.
- [qt-6.10.1-appimage-issues.md](./qt-6.10.1-appimage-issues.md) — concrete
  desktop breakage already measured on 6.10.1.
- [pure-rust-audio-backend.md](./pure-rust-audio-backend.md) — the NDK constraint.

Source PRD: `tasks/2026-07-27-131601-prd---android-api-36-compliance-and-packaging-follow-ups.md`.

---

## 1. Currently pinned versions

| Component | Pinned at | Why |
|---|---|---|
| Qt | 6.9.3 | Desktop, Windows, macOS and Android all ride on it |
| NDK | r26b / r27 (27.3.13750724 in use) | **Not r28** — at minSdk 27 its libc++ references `pthread_cond_clockwait` (bionic API 30+), which breaks the `cxx` C++ build |
| AGP | 8.6.0 | Coupled to the JDK pin and to Qt's `build.gradle` template |
| Gradle wrapper | 8.10 | **Ours**, checked into `android/gradle/wrapper/` — see §2.4 |
| JDK | 17–21 (`MAX_JDK_MAJOR=21` in `build-android.sh`) | AGP 8.6.0's bundled lint cannot parse a Java 26 version string |
| `compileSdk` | android-36 | Written by androiddeployqt; it picks the newest installed platform |
| `targetSdkVersion` | 36 | `android/build.gradle` `defaultConfig` |
| `minSdkVersion` | 27 | Overrides Qt's declared floor of 28 — see §2.2 |

---

## 2. Deferred work, to be picked up with the upgrade

### 2.1 Remove the predictive-back opt-out

**Status: workaround shipped in 1.0.0-alpha.3 (versionCode 4).**

`android/AndroidManifest.xml`'s `<activity>` carries:

```xml
android:enableOnBackInvokedCallback="false"
```

**Why.** targetSdk 36 enables the predictive back gesture with no per-app
opt-out at the *platform* level; the manifest attribute is the only escape
hatch. Predictive back stops the system dispatching legacy `KEYCODE_BACK` key
events and instead expects the app to register an `OnBackInvokedCallback`.

- **Qt 6.9.3 registers none.** Grepping Qt's Android Java sources
  (`~/Qt/<ver>/Src/qtbase/src/android/jar/src/org/qtproject/qt/android/`) finds
  **neither** `onBackPressed` **nor** `OnBackInvokedCallback`.
- **The app registers none either**, and deliberately so: Qt Quick Controls
  dismiss a `Dialog`/`Popup`/`Window` off the `Qt::Key_Back` event that the
  legacy path delivers. There is no `Key_Back` handler anywhere in
  `assets/qml/` — there never needed to be.

So with predictive back on, nothing anywhere handles back and the system default
runs: **finish the activity**. Measured on an Android 16 phone, 2026-07-28 —
back closed the entire app from the sutta reader (should open the tab list
dialog), from the tab list dialog, from the search help dialog, and from the
Chanting Practice window (each should have dismissed itself). Confirmed fixed by
the opt-out on the following build.

**What to do at upgrade time.** Check whether the new Qt implements
`OnBackInvokedCallback` (Qt bug tracker; look for back-handling in
`QtActivityDelegateBase` / `QtActivityBase`). If it does:

1. Remove the attribute.
2. **Re-test all four cases above.** Predictive back is not just a dispatch
   change — it adds a swipe animation with a commit/cancel phase, so a dialog
   that *closes* may still behave wrongly under a partial swipe.
3. If Qt handles the activity-level back but not Qt Quick popups, the app may
   need its own `OnBackInvokedCallback` bridging to QML. Do not assume Qt
   implementing the callback is sufficient.

Until then the attribute must stay. It is a genuine functional dependency, not
tidiness.

### 2.2 `minSdkVersion 27` vs Qt's declared floor of 28

**The common framing — that a 28 floor "arrives" with Qt 6.10 — is wrong.**

androiddeployqt already writes `qtMinSdkVersion=28` into the *generated*
`android-build/gradle.properties` on **Qt 6.9.3** (verified 2026-07-28), and
`android/build.gradle`'s `defaultConfig` overrides it back down to
`minSdkVersion 27`. The app has therefore shipped one API level below what the
current Qt declares it supports since the 6.9.3 move.

A Qt upgrade does not introduce the constraint; it removes our ability to keep
ignoring it.

**Decision (2026-07-29): raise `minSdkVersion` to 28 as part of the Qt
upgrade.** Not before — the override demonstrably works for shipped users on
Qt 6.9.3, so changing it on its own would drop API 27 (Android 8.1) devices for
no benefit. Doing it *with* the upgrade aligns the app with what Qt declares it
supports at the moment Qt's own floor becomes unavoidable.

When making the change:

- Edit `minSdkVersion` in `android/build.gradle`'s `defaultConfig` — that is the
  only source. There is no `<uses-sdk>` in `android/AndroidManifest.xml` and no
  `QT_ANDROID_TARGET_SDK_VERSION` in `CMakeLists.txt`.
- Verify with `aapt2 dump badging` (`minSdkVersion:'28'`), **not** by reading the
  generated `gradle.properties`.
- **Re-examine the NDK exclusion.** The r28 problem in §1 is specifically that
  its libc++ references `pthread_cond_clockwait`, which bionic declares only at
  API 30+ — that analysis is stated *at minSdk 27* and does not automatically
  change at 28 (still below 30), but it must be re-derived against whatever NDK
  the new Qt requires rather than assumed.
- Play will stop offering updates to devices below API 28. Existing installs on
  those devices keep the last compatible version; they are not uninstalled.

### 2.3 Deprecated Java APIs in Google Play's report

Play reports three deprecated APIs. **All three are in Qt's own Java, not
ours**, and are no-ops at API 36 (the system ignores status/navigation bar
colour calls under enforced edge-to-edge). Verified in the Qt 6.9.3 sources:

| API | File | Line |
|---|---|---|
| `Window.getStatusBarColor` | `QtActivityDelegateBase.java` | 108 |
| `Window.setStatusBarColor` | `QtDisplayManager.java` | 191, 200 |
| `Window.setNavigationBarColor` | `QtDisplayManager.java` | 192, 204 |

(Under `~/Qt/<ver>/Src/qtbase/src/android/jar/src/org/qtproject/qt/android/`.)

**We cannot remove these** without patching and rebuilding Qt's `Qt6Android.jar`,
which is not worth it for warnings about no-op calls. They will disappear when
Qt drops them. Expect the Play report to keep listing them until then — this is
the entry that stops the next Play report from restarting the investigation.

### 2.4 AGP, the Gradle wrapper and the JDK

The AGP bump belongs with the Qt upgrade, when Qt's `build.gradle` template
changes anyway. Three coupled facts:

1. **The Gradle wrapper is OURS, not Qt's.** `android/gradle/wrapper/` is
   checked into the repo at **8.10** (tracked since commit `f8eaafd`), and
   androiddeployqt copies it into `android-build/` along with the rest of
   `android/`. Qt 6.9.3's kits do ship a wrapper — at 8.12, in
   `~/Qt/6.9.3/android_*/src/3rdparty/gradle/gradle/wrapper/` — but **that copy
   is never used**. Verified 2026-07-28 by reading the generated
   `android-build/gradle/wrapper/gradle-wrapper.properties`.

   > Earlier notes in `AGENTS.md` claimed the wrapper was Qt's at 8.12 and used
   > that to argue an AGP bump would mean diverging from Qt. **That argument
   > does not hold** — the wrapper is ours to bump. AGP 8.10 needs Gradle
   > ≥ 8.11.1 and AGP 8.11+ needs 8.13, all reachable by editing our own file.

2. **The JDK pin is about AGP's bundled lint, and should survive any AGP bump.**
   AGP 8.6.0's lint cannot parse a Java 26 version string; `lintVitalAnalyzeRelease`
   dies with `> 26.0.1` as its *entire* error message — **after** all three ABIs
   have compiled and signed. The machine's default `java` is 26, which is why
   `build-android.sh` selects a JDK itself instead of inheriting one.

3. **`android/build.gradle` is a Qt-provided template** using `lintOptions`,
   `aaptOptions` and `packagingOptions` — deprecated through AGP 8.x and
   **removed in AGP 9.x**. Moving to AGP 9 means rewriting a file that has to be
   re-merged on every Qt upgrade. This is the real reason to couple the two.

Our own additions to that template (the `androidComponents { beforeVariants }`
debug-variant switch, the `packagingOptions.jniLibs.excludes` cross-ABI
workaround, the targetSdk comment) must be re-applied when the template is
re-merged.

### 2.5 The explicit 16 KB page-size link flag

`CMakeLists.txt:338` carries:

```cmake
target_link_options(simsapadhammareader PRIVATE "-Wl,-z,max-page-size=16384")
```

Qt 6.10 ships 16 KB page support out of the box, so this flag should become
redundant. **Do not remove it on faith** — verify with the sweep in §4 first;
it costs nothing to keep and its absence is silent until a device rejects the
library.

### 2.6 x86_64 and armeabi-v7a have still never been *run*

The multi-ABI work verified that both slices compile, link, package and carry
the right ELF machine type — nothing more. As of 2026-07-29 a Chromebook user
has confirmed the app **installs** (which closes the original "not compatible"
report and validates the x86_64 slice's packaging), but neither non-arm64 slice
has had a functional run.

Unchecked runtime risks: the pure-Rust audio stack (`cpal` AAudio backend,
`flacenc`, `rubato`, `symphonia`); tantivy index files on 32-bit armv7
(different `usize`; 32-bit is little-used upstream — x86_64 is if anything
*safer* than arm64, since the shipped index is built on x86_64 Linux); the JNI
paths (`android_saf.rs`, `ndk_context`, `android_helpers.cpp`); and Qt WebView /
Chromium under ARCVM.

A Qt upgrade changes all of these surfaces at once, so this is the moment to do
the runs that were skipped.

### 2.7 Re-examine `useLegacyPackaging`

`android/build.gradle:59` keeps `packagingOptions.jniLibs.useLegacyPackaging
true` — native libraries stored compressed and extracted to disk at install
time. The decision to keep it, with the full trade-off analysis, is in
[android-multi-abi-and-chromeos.md](./android-multi-abi-and-chromeos.md)
§ *`useLegacyPackaging` stays `true`*: the only benefit on offer is on-device
footprint, no current constraint is tight, and flipping it changes the loading
path of every native library.

It belongs with the upgrade for two reasons: the line is part of the
Qt-provided `build.gradle` template that has to be re-merged anyway, and the
"does every library still load on all three ABIs?" question it raises is the
same runtime validation §2.6 already demands. If it is flipped, measure both
ways — AAB/APK size, on-device install footprint, `zipalign -c -P 16`, and
`readelf -lW` `p_align` for the app `.so` and a Qt library.

---

## 3. Reasons to upgrade

- **Fixes the Gboard/Thai mid-word Shift bug** — the strongest *functional*
  reason on this list, because it makes non-Latin text entry work in every text
  field in the app. qtbase
  [`f5c0296fdaad`](https://code.qt.io/cgit/qt/qtbase.git/commit/?id=f5c0296fdaad1f4f824e9bd96c525000f658fa81)
  ("Android: Add support for GET_EXTRACTED_TEXT_MONITOR", 2025-10-08, `Fixes:`
  [QTBUG-140694](https://bugreports.qt.io/browse/QTBUG-140694),
  `Pick-to: 6.10 6.9 6.8`) stops Qt restarting the input connection on every
  keystroke:
  `QtInputConnection.java` goes from **12** `restartImmInput()` call sites in
  6.9.3 to **2** in 6.10.1, and `restartInput()` is what resets an IME's shift
  state. It landed 8 days after 6.9.3 was released, so our version just misses
  it. Not yet verified on device — verifying it is a reason to prioritise the
  upgrade. See
  [android-soft-keyboard.md §4](./android-soft-keyboard.md).
- Removes the deprecated `setStatusBarColor` / `setNavigationBarColor` calls from
  Play's report (§2.3).
- Possible removal of the predictive-back opt-out (§2.1) — the one item that is
  currently a *functional* workaround rather than cosmetic.
- Reported safe-area fixes for Android 16 (QTBUG-140193; safe areas reported
  working on 6.10.1+). We are not currently blocked on this — safe areas work
  correctly on 6.9.3 at targetSdk 36 — so it is insurance, not a fix.
- Qt 6.10 officially lists Android 9–16 support and ships 16 KB page support out
  of the box (§2.5).
- Newer AGP/Gradle templates, which is what unblocks §2.4.
- Staying on a Qt release that still receives fixes.

---

## 4. Pitfalls, from our own history

- **AppImage / libtiff.** Qt 6.10.1's `libqtiff.so` links `libtiff.so.5` while
  Arch ships `.so.6`; `linuxdeploy` fails. Current workaround: temporarily move
  `libqtiff.so` / `libqtga.so` aside during deployment. See
  [qt-6.10.1-appimage-issues.md](./qt-6.10.1-appimage-issues.md).
- **AppImage / WebEngine + FUSE.** Qt 6.10.1's WebEngine SIGSEGVs when run from a
  FUSE-mounted AppImage; the workaround is a wrapper forcing
  `--appimage-extract-and-run`, costing 2–5 s of startup and ~600 MB of temp
  space. **These two are measured, not theoretical, and are the reason 6.10.1
  was not adopted already.**
- **NDK.** A Qt upgrade changes the supported NDK, which changes the Rust/`cxx`
  build surface. Re-verify the `pthread_cond_clockwait` issue against the new
  minSdk (§2.2) rather than assuming the r28 exclusion still applies.
- **cxx-qt.** The bridge layer is version-sensitive; a Qt bump means re-verifying
  every `#[qinvokable]` bridge and the QML module registration.
- **The multi-ABI mechanism.** The per-ABI ExternalProject setup, the per-ABI Qt
  path guard, the cross-ABI plugin-staging bug and its
  `packagingOptions.jniLibs.excludes` workaround all depend on androiddeployqt
  behaviour and must be re-verified. The build script's cross-ABI check will
  catch a regression; the ELF and `zipalign` checks are **not** in the script and
  must be run by hand (§5).
- **Desktop, Windows and macOS ride the same Qt.** An Android-motivated upgrade
  is not an Android-only change.

---

## 5. Verification checklist for the upgrade

Static, per `build-android.sh` run:

```sh
make android-rebuild 2>&1 | tee /tmp/aab-build.log

# Debug variant must not be built during a release build
grep '^> Task .*[Dd]ebug' /tmp/aab-build.log
# (only :stripReleaseDebugSymbols and :mergeReleaseNativeDebugMetadata are OK —
#  both are release-variant tasks that merely contain the word)

# Target level: the ONLY trustworthy check. The generated gradle.properties
# still reads qtTargetSdkVersion=35, which build.gradle never reads.
aapt2 dump badging <apk> | grep -E "^package|SdkVersion|uses-permission|uses-feature|native-code|application-label"

# 16 KB alignment — NOT checked by build-android.sh; sweep every 64-bit lib,
# do not sample. armeabi-v7a at 0x1000 is correct (64-bit ABIs only).
readelf -lW <lib>.so | awk '/LOAD/{print $NF}' | sort -u
zipalign -c -P 16 4 <apk>
```

On device, the cases that actually broke before:

1. Back from the sutta reader, the tab list dialog, the search help dialog and
   the Chanting Practice window (§2.1).
2. Safe-area top inset: one inset, not zero and not doubled.
3. The `DrawerMenu` "Menu" label in portrait *and* landscape — a `Drawer` is in
   the window overlay and gets **no** Qt padding.
4. Soft keyboard raises on the **first** tap.
5. Audio record/playback, fulltext search, dictionary lookup, SAF file save —
   on a non-arm64 device if one can be found (§2.6).

---

## 6. Recommended shape of the upgrade

Upgrade **Android first on a branch with the desktop builds pinned**, verify the
full manual test plan, then move desktop/AppImage separately — the AppImage
problems in §4 are the ones most likely to stall the work, and they should not
block the Android benefits.

Change **one variable at a time**. The AGP/Gradle/JDK cluster (§2.4) and the Qt
bump each fail late and unhelpfully; doing both at once makes the failure
unattributable.
