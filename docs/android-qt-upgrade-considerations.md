# Android: Qt upgrade considerations

Simsapa is pinned to **Qt 6.9.3**. Several pieces of work were deliberately
*not* done during the July 2026 API 36 compliance push because they are blocked
on, or only sensible alongside, a Qt upgrade. This document records that
deferred work, the reasons to upgrade, and the pitfalls to guard against — so
the next person does not have to re-derive any of it.

> ## ⚠ Qt 6.10.3 was attempted for Android in August 2026 and REVERTED
>
> Read [§0](#0-the-august-2026-610.3-attempt-and-why-it-was-reverted) before
> planning another upgrade. In one sentence: **6.10.3 did not fix the bug it was
> undertaken for, and it broke the Android UI.** Everything else in this
> document still stands — including the build-version-correctness machinery,
> which was built during that attempt and is kept.

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

## 0. The August 2026 6.10.3 attempt, and why it was reverted

Source PRD: `tasks/2026-08-07-175059-prd---cxx-qt-and-qt-6-10-3-android-upgrade.md`
and its task list, which carry the full measurement trail.

The upgrade was **Android-only** (`QT_ANDROID` = 6.10.3, desktops left at
6.9.3), and it got as far as a signed multi-ABI release bundle before device
testing stopped it. `QT_ANDROID` is back at `6.9.3`.

### 0.1 It did not fix the Thai bug — the sole functional reason for it

The whole upgrade existed to fix the Gboard/Thai mid-word Shift bug
([android-soft-keyboard.md §4](./android-soft-keyboard.md)). On device
(Galaxy S23, Android 16), typing **รู้** on 6.10.3 behaves **exactly** as on
6.9.3: the long vowel is reachable only with shift-lock.

**The reasoning that led us here was wrong in an instructive way.** The PRD
inferred the fix from a **call-site count in a single file** — `restartImmInput()`
in `QtInputConnection.java`, 12 in 6.9.3 down to 2 in 6.10.x — plus the presence
of `GET_EXTRACTED_TEXT_MONITOR`. Both facts are true. The conclusion was not:

- The call on the keystroke path is in a **different file**,
  `QtEditText.onKeyDown()`, and is **byte-identical in 6.9.3 and 6.10.3**.
- Patching *that* out (verified live in the dex, not merely built) **still did
  not fix it**.
- A logcat capture across a Shift press shows **no `restartInput` at all**.
  What it does show is Gboard resetting itself:
  `LatinIme.resetInputContext(): reason=5, ExternalEditsInfo{… textLength=0,
  hasEdits=false}` — twice per key press, with the editor appearing **empty** to
  the IME even after text was typed.

So the "Qt restarts the input connection on every keystroke" story is **not a
sufficient explanation** of the bug. The live lead is that Qt's Android input
connection presents a synthetic, empty editor, and Gboard drops its context —
one-shot Shift included — because it cannot reconcile that. Firefox with the same
Thai layout on the same device behaves correctly (one Shift tap holds for exactly
one character), so the IME is fine; something in Qt's input handling cancels it.

**Rule this earns:** *a Qt version must be judged on device, not by reading its
sources.* Counting call sites in one file produced a confident, wrong prediction
that drove a multi-day upgrade.

### 0.2 It broke the Android UI — QtWebView was rearchitected in 6.10

Measured in the sources both kits ship:

| | 6.9.3 | 6.10.3 |
|---|---|---|
| `QQuickWebView` base class | `QQuickViewController` | **`QQuickWindowContainer`** |
| `quick/qquickviewcontroller.{cpp,_p.h}` | present | **deleted** |
| `webview/qnativeviewcontroller_p.h` | present | **deleted** |
| geometry/clip code in `qquickwebview.cpp` | active | **`#if defined(Q_OS_WASM)` only** |

On Android the native view is handed to the container wholesale
(`onNativeWindowChanged` → `nativeWindow->setParent(window())` +
`setContainedWindow(...)`), making it a native child window composited **above**
the Qt surface. Observed consequences, all from the one cause:

- A blank surface covers dialogs and dropdowns (the tab-list dialog's title was
  clipped mid-glyph at the surface's top edge) and ignores QML stacking.
- The search-info dialog appeared not to open — it was opening *behind* it.
- Drawer items and the mode/language selectors did not respond to taps.
- **Text entry broke app-wide**: the native WebView takes the
  `InputConnection`, so the IME serves it instead of Qt's `QtEditText`. The
  keyboard appears (Qt asked for it) but no character reaches the QML field —
  injected `adb shell input text` is swallowed identically.

**Proof it is the webview and nothing else:** a diagnostic build that swapped the
mobile webviews for stubs — same Qt 6.10.3 binary, no `WebView` instantiated —
restored text input, the Search Help dialog and the dropdowns simultaneously.

The app has long documented that Android's native view "renders in a separate
layer above Qt Quick content" (`SuttaHtmlView_Mobile.qml`); 6.10 made a
known-fragile area much worse.

### 0.3 What was kept

- **`QT_ANDROID` is back at 6.9.3**, and the diagnostic scaffolding (webview
  stubs, the patched `QtEditText`, the Gradle class-strip task) was removed.
- **The Qt-version-correctness machinery is kept in full** — see §0.4. It was
  built as Part C of that PRD and is independent of which version is targeted.
- **`list(APPEND app_components CorePrivate)`** in the Android branch of
  `CMakeLists.txt`. Redundant on 6.9.3, but it removes a hidden dependency: 6.9.3
  defines `Qt6::CorePrivate` as a *side effect* of `find_package(Qt6 COMPONENTS
  Core)` (`__qt_Core_always_load_private_module ON`), 6.10.3 does not, and the
  6.10.3 configure failed outright on it. Asking explicitly costs nothing and
  removes one failure from the next attempt.
- The corrected `armeabi-v7a` → `armv7-linux-androideabi` rationale in
  [android-multi-abi-and-chromeos.md](./android-multi-abi-and-chromeos.md).

### 0.4 The build-version-correctness system (built during the attempt, kept)

The attempt began by discovering that **the Linux build was not using the Qt it
claimed** — `find_package` resolved Arch's system Qt 6.11.1 while cxx-qt was
separately handed 6.9.3's qmake, so the binary mixed two Qt versions and the
AppImage bundled a third combination. That is fixed, and the machinery that
prevents its return is **the most durable result of this work**:

| Piece | What it guarantees |
|---|---|
| `CMakeLists.txt` `QT_*` variables | One declaration per platform; **everything else reads from here** |
| Linux `CMAKE_PREFIX_PATH` branch (`elseif (UNIX AND NOT APPLE AND NOT ANDROID)`) | `find_package` uses the declared kit, not ambient `PATH` |
| Post-`find_package` version assertion | A wrong Qt fails the **configure**, naming expected/found/`Qt6_DIR` |
| `qmake_path` derived from `Qt6_DIR` | The cxx-qt half cannot diverge from the CMake half |
| `scripts/qt-env.sh` | The single shell-side reader (`qt_version_for`), used by every build script |
| `scripts/qt-env-verify.sh` + `Invoke-EnvVerify` | Pre-flight gate on **every** build: reports the real toolchain and **stops** on a critical mismatch — including the kit's actual `qmake -query QT_VERSION` vs. the declared one |
| `build-android.sh` desktop-Qt scrub | Stops a `LD_LIBRARY_PATH`/`PATH` desktop kit being loaded by the Android host tools |

Full account: [qt-kit-selection.md](./qt-kit-selection.md).

**Why this matters more than the upgrade did.** With Android at 6.10.3 the
desktop and Android kits genuinely diverged for the first time, and the gate
caught real problems that were previously invisible — a stale `QT_ANDROID_VERSION`
export that would have silently built Android against the *desktop* kit, and a
foreign Qt on `LD_LIBRARY_PATH`. Those failure modes return the moment any future
divergence happens, so **keep the gate green even while all five platforms agree**;
it is dormant, not useless.

---

## 1. Currently pinned versions

| Component | Pinned at | Why |
|---|---|---|
| Qt | 6.9.3 | Desktop, Windows, macOS and Android all ride on it |
| NDK | r26b / r27 (27.3.13750724 in use) | **Not r28** — its libc++ references `pthread_cond_clockwait` (bionic API 30+), which breaks the `cxx` C++ build. The exclusion is about API **30**, so it held at minSdk 27 and holds unchanged at 28 |
| AGP | 8.6.0 | Coupled to the JDK pin and to Qt's `build.gradle` template |
| Gradle wrapper | 8.10 | **Ours**, checked into `android/gradle/wrapper/` — see §2.4 |
| JDK | 17–21 (`MAX_JDK_MAJOR=21` in `build-android.sh`) | AGP 8.6.0's bundled lint cannot parse a Java 26 version string |
| `compileSdk` | android-36 | Written by androiddeployqt; it picks the newest installed platform |
| `targetSdkVersion` | 36 | `android/build.gradle` `defaultConfig` |
| `minSdkVersion` | 28 | Matches Qt's declared floor, and is a hard requirement of `libQt6Core` — **raised 2026-08-26, decoupled from the upgrade**; see §2.2 |

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
  `bridges/assets/qml/` — there never needed to be.

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

### 2.2 `minSdkVersion` 28 — **done 2026-08-26, and decoupled from the upgrade**

**Status: shipped. This section is kept because the reasoning that moved it out
of this document is worth not re-deriving.**

`android/build.gradle`'s `defaultConfig` now declares `minSdkVersion 28`.
Verified on the artifact with `aapt2 dump badging`: `minSdkVersion:'28'`,
`targetSdkVersion:'36'`.

#### Why it was here, and why it left

Two framings were wrong, in sequence.

The first was that a 28 floor **"arrives" with Qt 6.10**. It does not.
androiddeployqt already writes `qtMinSdkVersion=28` into the *generated*
`android-build/gradle.properties` on **Qt 6.9.3** (verified 2026-07-28), and
`build.gradle` overrode it back down to 27. A Qt upgrade never introduced the
constraint; it would only have removed our ability to keep ignoring it.

The second framing is the one that put this section in a *deferred-work*
document, and it is the one that turned out to be a defect:

> **Decision (2026-07-29): raise `minSdkVersion` to 28 as part of the Qt
> upgrade.** Not before — the override demonstrably works for shipped users on
> Qt 6.9.3, so changing it on its own would drop API 27 (Android 8.1) devices
> for no benefit.

**The override did not work, and there were no such users.** A binary scan run
for the minSdk-28 PRD (2026-08-16) found that `libQt6Core_arm64-v8a.so` carries
a **GLOBAL** undefined `getentropy@LIBC_P` — a bionic symbol that first exists
at **API 28**. A GLOBAL undefined symbol is one the dynamic linker *must*
resolve, so on an Android 8.1 device `dlopen` fails before a line of app code
runs. The app was being offered by Play to devices on which it could never
start.

That makes the change a **correctness fix rather than a distribution
trade-off** — which is exactly the property that decoupled it from the Qt
upgrade and let it ship on its own, on Qt 6.9.3, with no version moves
anywhere. The measurement, the reproducible commands and the per-feature
inventory are in
[android-api-levels-and-feature-dependencies.md](./android-api-levels-and-feature-dependencies.md).

**The general lesson: "no technical risk, only a distribution cost, therefore no
benefit on its own" is a conclusion that needs a measurement.** Nobody had
scanned the binaries; the coupling to the Qt upgrade survived for a year on an
assumption.

#### What was confirmed at the time

- `android/build.gradle`'s `defaultConfig` is the **only** source of the floor —
  re-confirmed: no `<uses-sdk>` in `android/AndroidManifest.xml`, and no
  `*_SDK_VERSION` variable of any kind in `CMakeLists.txt`.
- `./scripts/android-api-scan.sh --floor 28` was run **once per ABI** against a
  multi-ABI APK. All three — `arm64-v8a`, `x86_64`, `armeabi-v7a` — report *"no
  hard (GLOBAL) undefined symbol requires more than API 28"*, so **28 is a
  sufficient floor, not merely a higher one**. The two non-arm64 slices had
  never been scanned before and neither raised the floor.
- **The NDK exclusion is unchanged and must not be re-litigated.** The r28
  problem is that its libc++ references `pthread_cond_clockwait`, which bionic
  declares only at API **30+**. 28 is still below 30, so the exclusion holds
  exactly as before. It must still be re-derived against whatever NDK a *new Qt*
  requires — but not because of this change.
- `WRITE_EXTERNAL_STORAGE` reasoning is unaffected: the app stays inside the
  API 27–28 band where the permission would be required, and it still never
  touches shared external storage (everything goes through SAF `content://` URIs
  or the app-private directory).

#### User-facing consequence

Play stops offering updates to devices below API 28. **Existing installs on
those devices keep the last compatible version; they are not uninstalled.**
Sideloaded APKs from GitHub Releases likewise refuse to install on API 27.

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

- ~~**Fixes the Gboard/Thai mid-word Shift bug**~~ — **DISPROVEN ON DEVICE
  2026-08-08. This was the strongest functional reason on the list; it is now
  gone.** The app was built against the 6.10.3 Android kit and tested with
  Gboard Thai on a Galaxy S23: typing **รู้**, the mid-word Shift still reverts
  immediately and shift-lock is still required — indistinguishable from 6.9.3.
  The reasoning below counted `restartImmInput()` in `QtInputConnection.java`
  only, and **missed `QtEditText.onKeyDown()`**, which calls it on every key
  down and is **byte-identical in 6.9.3 and 6.10.3**. `f5c0296fdaad` is
  necessary but not sufficient. A patched-`QtEditText` route that would work on
  **6.9.3, with no Qt upgrade at all**, is sketched in
  [android-soft-keyboard.md §4](./android-soft-keyboard.md).

  Superseded reasoning, kept because the sources it cites are accurate and only
  the conclusion was wrong:

  qtbase
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
  build surface. Re-verify the `pthread_cond_clockwait` issue against whatever
  NDK the new Qt requires, rather than assuming the r28 exclusion still applies.
  Note the floor itself is settled at 28 (§2.2) and is no longer a variable
  here; what a new NDK can change is the *symbol* requirement. Re-run
  `./scripts/android-api-scan.sh --floor 28` per ABI after the bump — the scan
  is cheap and it is what caught the `getentropy` defect.
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

**Do the on-device pass FIRST, on a debug APK, before spending a day on AGP,
Gradle and packaging.** (minSdk is no longer on that list — it was raised to 28
on its own in August 2026, §2.2.) The 6.10.3 attempt did those in the documented
order — packaging work, then device — and every hour of it was wasted, because
the device pass would have killed the upgrade on the first screen. The cheapest
build that can be typed into is worth more than a signed bundle.

**Two new must-check items, both learned the hard way (§0):**

0a. **Does the version actually fix the bug you are upgrading for?** Verify the
    *symptom* on device, never the source. Reading Qt's sources produced a
    confident, wrong prediction about the Thai fix.

0b. **Do the webview panels still compose with the QML scene?** Open a sutta,
    then a dialog and a dropdown over it, and toggle the sidebar. If a blank
    surface covers them — or text entry stops working anywhere in the app — the
    QtWebView native-window integration has regressed (§0.2). A fast way to
    confirm attribution: build with the mobile webviews stubbed out; if every
    unrelated symptom disappears at once, it is the webview.

On device, the cases that actually broke before:

1. Back from the sutta reader, the tab list dialog, the search help dialog and
   the Chanting Practice window (§2.1).
2. Safe-area top inset: one inset, not zero and not doubled.
3. The `DrawerMenu` "Menu" label in portrait *and* landscape — a `Drawer` is in
   the window overlay and gets **no** Qt padding.
4. Soft keyboard raises on the **first** tap, and does not flash off and on
   when a tap moves the cursor in an already-focused field.
5. The blue cursor handle does **not** stay on screen after closing a window
   with a focused `TextArea`. The Qt 6.9.3 workaround is one block per window
   ([android-soft-keyboard.md §5](./android-soft-keyboard.md)); check whether
   the upgrade makes it unnecessary.
6. Audio record/playback, fulltext search, dictionary lookup, SAF file save —
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

**Revised after the 6.10.3 attempt — put the two cheap kill-switches first:**

1. **Bump `QT_ANDROID`, build a debug APK, install it, and use the app.** Nothing
   else. Text entry, a dialog, a dropdown, a sutta tab. This is ~1 hour and it is
   what would have ended the 6.10.3 attempt on day one.
2. **Verify the motivating symptom is actually fixed**, on device.
3. Only then the packaging work: AGP/Gradle/JDK, 16 KB, multi-ABI AAB. (minSdk
   is already at Qt's declared floor of 28 and needs nothing here.)

Steps 1 and 2 are the *reason* for the upgrade; steps in §2 are the *cost*. The
2026-08 attempt paid the cost before checking the reason.

**On choosing a target version:** 6.10.3 was picked as the conservative step
(2-line Gradle template delta, no Kotlin plugin, stays inside AGP 8.x) and that
reasoning was sound — the template really is nearly identical. It says nothing
about runtime behaviour, which is where it failed. Check the QtWebView Android
integration (§0.2) in any candidate version *before* adopting it: whether
`QQuickWebView` still derives from `QQuickWindowContainer`, and whether that path
has since learned to respect QML stacking, visibility and input.
