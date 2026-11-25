# The patched Qt for Android kit

Simsapa's Android packages are built against a **patched Qt 6.9.3 kit**, not
the stock kit the Qt online installer puts in `~/Qt/6.9.3`. This document says
why, what is patched, how the kit is built, how the build picks it up, and how
each package is checked for it.

Short version:

``` sh
make android-qt-kit          # once per machine (and after a patch change): ~2 min per ABI
make android-aab             # or any other android-* target; uses the patched kit
```

`build-android.sh` refuses a kit without the patches unless
`SIMSAPA_ALLOW_STOCK_QT=1` is set, and every package built with the patched
kit has to pass a verify gate before the build reports success.

## 1. Why

Stock Qt 6.9.3 has these Android keyboard bugs, all on a Galaxy S23 with
Android 16 and Gboard, and all fixed by the patch set:

| Bug | Fixed by |
|---|---|
| Gboard's one-shot **Shift is lost mid-word**: the Thai shifted layer (e.g. the vowel in รู้) can only be typed with shift-lock ([android-soft-keyboard.md](./android-soft-keyboard.md) §4) | `0005` (Qt-local), with `0001`–`0003` for landscape / fullscreen mode |
| The keyboard needs **two taps** to come up (the first `show()` is dropped) ([android-soft-keyboard.md](./android-soft-keyboard.md) §1) | `0003` (upstream `5fd393cb69be`, QTBUG-140897) |
| Every `show()` while the keyboard is up makes the system restart Gboard's input view: the keyboard **flashes** on a tap in a focused field and at every focus change | `0003` |
| IME calls are dropped while Qt thinks the keyboard is hidden: a composing word is lost on rotation, and fullscreen mode is a race | `0001` (upstream `491ac58e3d24`, QTBUG-140856) |
| In fullscreen (landscape extract) mode, Qt restarts the IME on every edit | `0002` (upstream `f5c0296fdaad`, QTBUG-138858 / QTBUG-37980) |
| A long press on the first word of a field can read past the start of the text (undefined behaviour) | `0004` (upstream `83876bf5500b`, QTBUG-148434) |

Qt 6.10.3 was tried in August 2026 and does **not** fix the Thai Shift bug
([android-qt-upgrade-considerations.md](./android-qt-upgrade-considerations.md)
§0.1): it has `0001`/`0002`, but still ends the composition on modifier keys
(`0005`). Qt `dev` still does both per-key things `0005` removes (read from
the source, not tested), so a Qt upgrade alone does not remove the need for
this kit.

The root-cause analysis, the logcat evidence and the device tests were done in
a separate research repo (`qt-thai-input-issue-example`, kept beside this one
in the author's tree; not needed to build). Its conclusions are summarized
here and in [android-soft-keyboard.md](./android-soft-keyboard.md) §4.

**Not fixed by the kit:** QTBUG-151926, a long press in a Qt Quick `TextArea`
loses its selection on release. That is in qtdeclarative, not qtbase; Simsapa
works around it in QML ([android-soft-keyboard.md](./android-soft-keyboard.md)).

## 2. The patches

In `android/qt-patches/`, as `git format-patch` files, applied in order to a
pristine copy of `~/Qt/6.9.3/Src/qtbase`:

| File | What | Upstream |
|---|---|---|
| `0001-…hasValidF.patch` | C++: don't check keyboard visibility in `QAndroidInputContext::hasValidFocusObject()` | backport of `491ac58e3d24` |
| `0002-…GET_EXTRACTED_TEXT_MONITOR.patch` | Java: `QtInputConnection` updates the extracted text instead of restarting the IME; `m_isComposing` guard in `setSelection` | backport of `f5c0296fdaad`, ported by hand (the file layout differs), same changed lines |
| `0003-…setWindowInsetsAnimationCallback….patch` | Java: `QtInputDelegate.showKeyboard()` no longer takes control of the IME insets animation | backport of `5fd393cb69be` |
| `0004-…longPress….patch` | C++: iterator fix in `QAndroidInputContext::longPress()` | backport of `83876bf5500b` |
| `0005-…modifier keys.patch` | Java: `QtInputConnection.sendKeyEvent()` does not finish the composition, and `QtEditText.onKeyDown()` does not restart the IME, for a modifier key (`KeyEvent.isModifierKey()`) | **Qt-local**; to be reported upstream |
| `0006-…DO-NOT-MERGE-add-build-marker….patch` | Java + C++: the string `qt-thai-fix-marker-v1`, logged once when an input connection / input context is created | Simsapa-only: lets the verify gate prove the package carries the patched code |
| `debug/0001-debug-input-logging.patch` | temporary logging of every IME call (tag `QtThaiDbg`) | **never** in a distributed build; `build-kit.sh --with-debug` |

The marker costs one `Log.d` line per input connection and one `qCInfo` in
the `qt.qpa.input.methods` category, which is off by default. Shipping it is
deliberate.

Only two kit files change: `jar/Qt6Android.jar` and
`plugins/platforms/libplugins_platforms_qtforandroid_<abi>.so`. The plugin has
the same exported symbols as the stock one; its only new import is
`QMessageLogger::info` (the marker's `qCInfo`), which the stock `Qt6Core`
exports. So the rest of the stock kit (Core, Gui, Quick, …) is used as is.

## 3. Building the kit

Prerequisites: Qt 6.9.3 from the online installer with the **Sources**
component and the **Android** kits for arm64-v8a, x86_64 and armeabi-v7a (the
same as for a stock build), the Android SDK and NDK `27.3.13750724` (as
`build-android.sh`), a JDK 17–21, `ninja`, `rsync`, `git`.

``` sh
make android-qt-kit                                   # all three ABIs
KIT_ABI=x86_64 ./scripts/qt-patched-kit/build-kit.sh  # one ABI
```

The scripts are in `scripts/qt-patched-kit/`:

- `env.sh`: shared settings (Qt version from `CMakeLists.txt`'s `QT_ANDROID`,
  paths, the ABI list).
- `prepare-src.sh`: copies `~/Qt/<version>/Src/qtbase` to `../qt-work/qtbase-src`
  (first run), makes it a git repo tagged `pristine-<version>`, and on every run
  resets branch `patched` to that tag and `git am`s `android/qt-patches/`.
- `build-kit.sh`: configures qtbase for each ABI in `../qt-work/qtbase-build-android-<abi>`
  with the stock kit's options (`-release`, `-android-abis <abi>`,
  `-qt-host-path ~/Qt/<version>/gcc_64`; no OpenSSL, which only QtNetwork uses
  and which is not swapped), builds only `Qt6Android` (the jar) and
  `QAndroidIntegrationPlugin` (the plugin), copies the stock kit for that ABI
  to the kit root, swaps the two files in (the plugin stripped, as installed),
  relocates the copy, and writes `PATCHED-KIT-MANIFEST-<abi>.txt`. It runs the
  build in a `systemd-run --user --scope` with `MemoryMax` (`MEMORY_MAX`,
  default 12G) and `-j` (`JOBS`, default 4), and stops if `clangd`/`jdtls` is
  running (it would index qtbase beside the build). Safe to re-run.
- `relocate-kit.sh`: rewrites the one file in a copied kit that names the
  stock kit's own path (`lib/cmake/Qt6BuildInternals/QtBuildInternalsExtra.cmake`)
  and checks that nothing else does and that `qmake -query` stays inside the
  copy. The installed kit is otherwise relocatable (`qt-cmake` and
  `qt.toolchain.cmake` find their paths relative to themselves).
- `verify-package.sh`: the verify gate, see §5.

The first build of an ABI compiles Core and Gui (the plugin links against
them): about 2 minutes at `-j4`. A rebuild after a patch change takes seconds.
Nothing is ever written under `~/Qt`.

### Layout

Beside the Simsapa repo (`<parent>` is the folder that holds `simsapa/`):

```
<parent>/qt-work/qtbase-src/                 patched qtbase (git; branch `patched`)
<parent>/qt-work/qtbase-build-android-{arm64,x86_64,armv7}/
<parent>/qt-6.9.3-thai-fix/                  the kit (QT_ANDROID_ROOT)
    android_arm64_v8a/  android_x86_64/  android_armv7/   copies of the stock kits, 2 files swapped
    gcc_64 -> ~/Qt/6.9.3/gcc_64                           the stock host kit, symlinked
    PATCHED-KIT-MANIFEST-<abi>.txt                        patches, and per swapped file: path, stock sha256, patched sha256
```

The kit takes about 2 GB (three Android kit copies); the work dirs about 1.2 GB.
All three ABIs sit in one root because Qt's multi-ABI build looks for an extra
ABI's kit as a **sibling** of the primary kit (`<kit>/../android_<abi>`,
`_qt_internal_get_android_abi_prefix_path` in `Qt6AndroidMacros.cmake`).

## 4. How the build picks it up

- `build-android.sh` defaults `QT_ANDROID_ROOT` to `../qt-<QT_ANDROID>-thai-fix`
  and runs that root's `android_<primary ABI>/bin/qt-cmake`. The header prints
  `Qt kit: patched (default: patched kit beside the repo)`.
- **Patched or stock is decided by the manifests**, not the path: every ABI
  being built needs `PATCHED-KIT-MANIFEST-<abi>.txt` in `QT_ANDROID_ROOT`.
  Otherwise the build stops, unless `SIMSAPA_ALLOW_STOCK_QT=1`:

  ``` sh
  QT_ANDROID_ROOT=$HOME/Qt/6.9.3 SIMSAPA_ALLOW_STOCK_QT=1 make android-beta-debug-arm64
  ```

  A stock build prints a warning, skips the verify gate, and must not be
  distributed.
- If the default kit is missing, the build stops and says `make android-qt-kit`.
- **A build directory remembers its kit.** CMake keeps the
  `CMAKE_TOOLCHAIN_FILE` of an existing build directory and ignores a new one
  with only a warning, so switching kits in the same directory would silently
  keep the old kit. `build-android.sh` compares the cached toolchain with the
  one it wants and stops; remove the directory with `--clean`,
  `make android-clean` or `make android-arm64-clean`.
- `CMakeLists.txt` takes the kit from the `qt-cmake` toolchain (which always
  prepends its kit to `CMAKE_PREFIX_PATH`); the `~/Qt/<version>/android_<abi>`
  paths there are only a fallback for a configure without `qt-cmake`. The kit
  actually used is the `-- Qt 6.9.3 (expected 6.9.3) at <Qt6_DIR>` line and
  `-- Using qmake: …` (the qmake handed to `cxx_qt_import_crate`).
- Multi-ABI: each extra ABI's sub-build is configured with that ABI's
  toolchain from the same root, so it uses the patched `android_x86_64` /
  `android_armv7` (checked: `CMAKE_TOOLCHAIN_FILE`, `Qt6_DIR` and qmake in
  `android_abi_builds/<abi>/CMakeCache.txt`).
- **The About dialog shows the kit**, as `Qt Version: 6.9.3, Qt kit: thai-fix`
  (or `stock`). `CMakeLists.txt` sets `SIMSAPA_QT_KIT` by the same manifest
  test, per ABI (`<kit prefix>/../PATCHED-KIT-MANIFEST-<ANDROID_ABI>.txt`),
  prints `-- Qt kit: …`, and compiles it into `get_qt_kit()` in
  `cpp/utils.cpp`. Desktop builds are always `stock`.
- `scripts/qt-env-verify.sh` checks the kits and the host kit under
  `QT_ANDROID_ROOT` (or the same default).

See also [qt-kit-selection.md](./qt-kit-selection.md) and
[android-multi-abi-and-chromeos.md](./android-multi-abi-and-chromeos.md).

## 5. The verify gate

After packaging, `build-android.sh` runs

``` sh
PATCHED_KIT_ROOT=$QT_ANDROID_ROOT EXPECT_ABIS=$ANDROID_ABIS \
  scripts/qt-patched-kit/verify-package.sh <apk|aab> --expect-patched
```

and fails the build on any FAIL. It reads an APK (`classes*.dex`,
`lib/<abi>/`) or an AAB (`base/dex/`, `base/lib/<abi>/`) and checks:

1. `org.qtproject.qt.android.QtInputConnection` is defined exactly once in the
   dex files. A second copy (e.g. a patched class put in `android/src/`) would
   make it undefined which one runs; Gradle silently prefers the jar's.
2. The marker `qt-thai-fix-marker-v1` is in the dex.
3. For every packaged ABI (and every ABI in `EXPECT_ABIS`): the packaged
   `libplugins_platforms_qtforandroid_<abi>.so` has the patched sha256 from
   that ABI's manifest (Gradle packages the stripped kit plugin byte for byte).
4. The marker is in each plugin.
5. Call sites that only the fixes produce, per class and method, from
   `dexdump -d`: `QtInputConnection.updateFullScreenExtractedText` calls
   `InputMethodManager.updateExtractedText` (`0002`);
   `QtInputDelegate.showKeyboard` calls `setWindowInsetsAnimationCallback` and
   not `controlWindowInsetsAnimation` (`0003`);
   `QtInputConnection.sendKeyEvent` and `QtEditText.onKeyDown` call
   `KeyEvent.isModifierKey` (`0005`). So a kit with only the marker cannot
   pass. The C++ fixes (`0001`, `0004`) leave no reliable pattern in the
   stripped plugin; check 3 covers them.

It works because R8 is off ([android-multi-abi-and-chromeos.md](./android-multi-abi-and-chromeos.md));
if R8 is ever turned on, the Qt classes must be kept unrenamed or the gate
must be revised. Run it by hand on any package with `--expect-patched` or
`--expect-stock`. The jar's sha256 changes from build to build (zip
timestamps), which is why the jar is checked by its classes, not its checksum.

## 6. Carrying the patches to another Qt version

1. Set the new `QT_ANDROID` in `CMakeLists.txt` and install that Qt with
   Sources and the three Android kits.
2. Drop the backports the new version already has: `0001` (in 6.10.x),
   `0002` (verified in 6.10.1+), `0003` (check the version's
   `QtInputDelegate.showKeyboard()`), `0004` (check `longPress()`). Read each
   against the new source; do not go by version numbers alone.
3. `make android-qt-kit`. A patch that no longer applies stops
   `prepare-src.sh`; fix it in `../qt-work/qtbase-src` on branch `patched`,
   then `git format-patch pristine-<version>..patched -o android/qt-patches/`
   (in the work tree) and keep the numbering. The marker patch stays last.
4. Check that the patches still touch only the jar and the platform plugin
   (`build-kit.sh` stops otherwise: a change in Core or Gui needs more files
   swapped and an ABI re-check).
5. Update the fix signatures in `verify-package.sh` (check 5) if the code
   they match changed.
6. Re-test on a device: the Thai Shift procedure in
   [android-soft-keyboard.md](./android-soft-keyboard.md) §4, the first tap,
   a tap in a focused field (no flash), rotation with a composing word,
   long press.

Once upstream Qt fixes the modifier-key bug itself, the kit can go: set the
default `QT_ANDROID_ROOT` back to `~/Qt/<version>` and remove this pipeline.

## 7. Verified (2026-11-25)

Galaxy S23, Android 16, Gboard 18.6.3, Simsapa 1.3.3 on the patched kit:

- `make android-beta-debug-arm64`: the Thai Shift procedure (`ร`, one Shift
  tap, `ู`, `้` → รู้, then the base layer) in the search bar and the gloss
  input, 3× each; US typing and suggestions, Backspace over a Thai cluster,
  Enter, focus switching, rotation with a composing word; the keyboard comes
  up on the first tap and does not flash. All pass.
- The same with the recording marker comment field after it got the helper.
- `make android-aab`: verify gate PASS for all three ABIs on the bundle.
- `make android-beta-dist` (release type, not debuggable, three ABIs): gate
  PASS; clean install, first-run setup, the Shift procedure: pass.

Not tested on hardware: x86_64 (Intel/AMD Chromebooks) and armeabi-v7a; the
gate checks that they carry the patched plugin. Not tested: Android 9/10
(where `0003` does not act), other keyboards than Gboard, iOS (not affected:
the patches are Android-only).
