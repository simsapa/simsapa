# Pure-Rust audio backend (16 KB compliance)

The chanting-practice recorder and player use a **pure-Rust audio stack**
instead of Qt Multimedia (which pulled in Qt's FFmpeg backend and its five
4 KB-aligned `libav*`/`libsw*` prebuilts). This doc covers the Android build
implications; the recorder/player architecture is expanded as the stack is
finalized.

## Crates

- **`cpal`** (0.18) — audio capture (input) and playback (output) streams.
- **`flacenc`** — encode user recordings to FLAC.
- **`rubato`** — resample to the canonical recording format.
- **`symphonia`** — decode FLAC (user recordings) and MP3 (shipped reference
  recordings) for playback and waveform rendering.

There is **no `oboe`** dependency: cpal 0.18 dropped the `oboe` crate and its
Android backend is now **AAudio via the `ndk` crate** (deps `ndk` /
`ndk-context` / `jni`; no `oboe-sys`). Consequences:

- No C++ is compiled from source for audio, and **no audio native library is
  bundled** in the APK — AAudio is a system NDK library.
- The main app `.so` is the only artifact *we* link that needs 16 KB page
  alignment; Qt's own prebuilt libs are already 16 KB-aligned.

## NDK version: stay on r27 (do NOT use r28)

16 KB page alignment (required for Play Store submissions targeting API 35+)
would be the *default* under NDK r28, but **NDK r28 is incompatible with this
project's Qt Android build at its `minSdkVersion`**:

- r28's libc++ `condition_variable.h` references `pthread_cond_clockwait`, which
  bionic declares only at **API 30+**. The `cxx` crate's C++ (`cxx.cc`) in the
  `bridges` build then fails with `use of undeclared identifier
  'pthread_cond_clockwait'`.
- Qt 6.9.3's supported NDK is **r26b**; r28 is not validated against it.
- A pure-Rust `cargo build --target aarch64-linux-android` does **not** reveal
  this — the backend has no C++. Only the `bridges` (cxx) C++ trips it, i.e. a
  full Qt/Corrosion Android build.

**The exclusion is not tied to minSdk 27, and did not lapse at 28.** Bionic
gained `pthread_cond_clockwait` only at API 30, so the raise of `minSdkVersion`
to 28 (done on **2026-08-26**, on its own — *not* as part of the Qt 6.10.3
upgrade, which was abandoned and reverted) changed nothing here. Raising it to
30 to satisfy r28 would drop Android 8–10 devices, so that is not the fix
either.

### The NDK is pinned explicitly

`build-android.sh` pins **`ANDROID_NDK_VERSION=27.3.13750724`** (r27d, clang
18.0.4) rather than selecting one. It previously took the **highest installed**
NDK (`ls … | sort -V | tail -1`), which meant `sdkmanager` installing a newer NDK
for any unrelated reason silently swapped this project's compiler — and Qt's own
auto-detect behaves the same way (`QtAutoDetectHelpers.cmake` sorts descending
and takes `[0]`), so nothing downstream would have corrected it.

**The NDK is a non-variable across the Qt upgrade.** Qt 6.9.3 *and* 6.10.3 were
both built against NDK **27.2.12479018** (`modules/Core.json` in each kit), so
the Qt bump does not ask for an NDK change. We stay on 27.3.13750724 because that
is what shipped 1.0.0 to Play — the only known-good data point. Do **not** align
it down to 27.2 to match Qt exactly: a clean build would not prove the downgrade
safe (most NDK problems are loud, but codegen differences and runtime-resolved
paths — cpal/AAudio, JNI, unwinding — are not), and 27.2 is more useful held in
reserve as a single-variable diagnostic lever.

`ANDROID_NDK_VERSION` and `ANDROID_NDK_ROOT` both remain overridable; the run
header reports which one decided. The `ndk_major >= 28` guard stays as a backstop
for exactly those overrides.

### How 16 KB alignment is achieved instead

Stay on the pinned NDK (see above; propagated into Gradle by `ndkVersion
androidNdkVersion` in `android/build.gradle`) and set the page size explicitly on
the main app `.so`:

```cmake
# CMakeLists.txt, ANDROID branch
target_link_options(simsapadhammareader PRIVATE "-Wl,-z,max-page-size=16384")
```

The `-Wl,-z,max-page-size=16384` flag works on any NDK ≥ r22 and produces an ELF
with `p_align == 0x4000`. Combined with Qt's already-aligned libs and the absence
of any bundled audio native lib, the APK passes the 16 KB checks.

### Verification

```sh
# ELF LOAD-segment alignment of the main app lib (want 0x4000 = 16 KB)
unzip -p app.apk lib/arm64-v8a/libsimsapadhammareader_arm64-v8a.so > /tmp/x.so
readelf -lW /tmp/x.so | grep LOAD          # last column is p_align

# APK-level 16 KB page alignment
$ANDROID_SDK/build-tools/<ver>/zipalign -c -P 16 4 app.apk && echo PASS

# Confirm the five FFmpeg libs are gone (no output = good)
unzip -l app.apk | grep -E 'libav|libsw'
```

## Android link: NDK system libraries

cpal's AAudio backend and the `ndk` / `ndk-sys` crates reference Android system
APIs and declare the libraries they need via `#[link(name = "…")]` attributes.
Those attributes are honoured **only when rustc drives the final link**. In this
project the Rust code is built as a **staticlib** (`libsimsapa_bridges.a`) and
CMake/Corrosion links it into the app `.so` with clang++, so the `#[link]`
directives are dropped and the link fails with undefined symbols
(`AAudio_*`, `ASharedMemory_*`, `ANativeWindow_*`, `ALooper_*`, …).

The fix is to name the libraries explicitly in the `ANDROID` branch of
`CMakeLists.txt`:

```cmake
target_link_libraries(simsapadhammareader PRIVATE android nativewindow aaudio)
```

| Library        | Symbols pulled in                                   |
| -------------- | --------------------------------------------------- |
| `android`      | `ASharedMemory_*`, `ATrace_*`, `AInputQueue_*`, `ALooper_*` |
| `nativewindow` | `ANativeWindow_*`                                   |
| `aaudio`       | cpal AAudio capture/playback (`AAudio_*`)           |

All three are stub libraries in the NDK sysroot; naming ones we don't strictly
use is harmless.

## Desktop links: what the final link must supply

The same staticlib rule applies on every platform whose static archive format
carries no link directives (Mach-O and ELF `.a`). Each platform uses a
different cpal backend module, so each needs its own entry in `CMakeLists.txt`:

| Platform | cpal backend | `CMakeLists.txt` names |
|---|---|---|
| Android | AAudio (`ndk`) | `android nativewindow aaudio` |
| Linux | ALSA | `asound` (via pkg-config `alsa`) |
| macOS | CoreAudio (`coreaudio-rs`, `objc2-core-audio`) | `-framework CoreAudio`, `-framework AudioToolbox` |
| Windows | WASAPI | nothing audio-specific (only `bcrypt`, for getrandom) — verified November 2026: the Windows build links and runs. If a later cpal / `windows` crate upgrade fails the link on audio symbols, this row is the place to fix |
| iOS (dormant) | RemoteIO + `AVAudioSession` (`host/coreaudio/ios/`) | **not implemented** — a revived iOS build will need `AudioToolbox` + `AVFAudio` (or `AVFoundation`), **not** `CoreAudio`; cpal's iOS module never touches the macOS HAL. The `elseif (IOS)` branch of `CMakeLists.txt` links neither today |

macOS was missing its row from the switch away from Qt Multimedia until
November 2026: the link failed with 19 undefined symbols (`AudioObject*`,
`AudioHardware*` from CoreAudio; `AudioUnit*`, `AudioComponent*`,
`AudioOutputUnit*` from AudioToolbox). Nothing caught it because there is no
macOS CI — build on the Mac before a release.

## macOS minimum version: 14.2

`CMAKE_OSX_DEPLOYMENT_TARGET` (and `LSMinimumSystemVersion`, which
`build-macos.sh` reads from that same line) is **14.2**, the higher of:

- **12.0** — Qt 6.9.3's frameworks are built with `minos 12.0`;
- **14.2** — cpal 0.18's loopback module (`host/coreaudio/macos/loopback.rs`,
  declared unconditionally, no cargo feature to drop it) calls
  `AudioHardwareCreateProcessTap` / `AudioHardwareDestroyProcessTap`
  (`API_AVAILABLE(macos(14.2))`). `objc2-core-audio` links them **strongly**:

```
$ nm -m simsapadhammareader.app/Contents/MacOS/simsapadhammareader | grep -i processtap
                 (undefined) external _AudioHardwareCreateProcessTap (from CoreAudio)
                 (undefined) external _AudioHardwareDestroyProcessTap (from CoreAudio)
```

`external` (not `weak external`) means dyld refuses to launch the app on a
macOS without them. cpal only *calls* them when opening an input stream on an
output-only device, which the recorder never does, so weak-linking CoreAudio
(`-weak_framework CoreAudio`) with a 12.0 floor would be possible — it was
considered and not chosen, since no pre-14.2 Mac is available to test it.
Re-run the `nm` check after any cpal / objc2-core-audio upgrade before
changing the floor in either direction. A Qt upgrade can raise Qt's own floor
too: check `otool -l` on a Qt framework (`LC_BUILD_VERSION` → `minos`) and take
the higher of the two.

**What has and has not been tested.** Verified on macOS 26.6.2 (November
2026): the clean build has zero `ld: warning:` lines, the app installed from
the DMG launches, recording and playback in Chanting Practice work, the
microphone prompt appears, and the installed `Info.plist` reads 14.2. That
machine **cannot** test behaviour on older macOS: every symbol in question
exists on 26.x. That the 14.2 floor is right and sufficient rests on the `nm`
evidence above, not on a launch on 14.2 itself.

**Cargo must get the same target.** Corrosion does not forward
`CMAKE_OSX_DEPLOYMENT_TARGET`, so without
`corrosion_set_env_vars(simsapa_bridges "MACOSX_DEPLOYMENT_TARGET=…")` rustc and
cc-rs build the staticlib for rustc's default (11.0). That broke the link once
the CMake side moved to 14.2: libc++'s headers choose by deployment target
whether `basic_stringbuf::str()` is instantiated inline or imported from
`libc++.dylib`, so CMake's `gui.cpp.o` referenced it undefined, the linker
pulled the staticlib's own `gui.o` (cxx-qt compiles `cpp/gui.cpp` there too) to
satisfy it, and 36 duplicate symbols followed. `ld -why_load` names the
culprit symbol.

## Android JNI context (`ndk_context`) — required, or playback aborts

cpal's AAudio backend reaches the Java side over JNI (e.g. reading the
`aaudio.mixer_bursts` system property while building a stream). It obtains the
`JavaVM` and Activity `Context` from **`ndk_context::android_context()`**. Apps
built with `cargo-apk` / `ndk-glue` initialize `ndk_context` automatically, but
**this app uses Qt for Android**, which has its own entry point and never does.

With `ndk_context` uninitialized, `context.vm()` is null and jni's
`JavaVM::from_raw` asserts non-null → **panic inside the cxx-qt queued closure →
`cxx::unwind::prevent_unwind` aborts** (SIGABRT) the first time an audio stream
is built. The crash backtrace shows `panic_in_cleanup` →
`prevent_unwind` → `…AudioManager_cxxQtThreadQueue…`.

Fix: initialize `ndk_context` once at startup with Qt's JavaVM and a JNI **global
ref** to the Activity context. C++ side (`cpp/gui.cpp`, after `QApplication` is
constructed):

```cpp
#ifdef Q_OS_ANDROID
  JavaVM* java_vm = QJniEnvironment::javaVM();
  QJniObject activity = QNativeInterface::QAndroidApplication::context();
  QJniEnvironment env;
  jobject global_context = env->NewGlobalRef(activity.object());
  init_android_context(java_vm, global_context);
#endif
```

Rust side (`backend/src/lib.rs`, Android-only `extern "C"`):

```rust
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn init_android_context(java_vm: *mut c_void, context: *mut c_void) {
    unsafe { ndk_context::initialize_android_context(java_vm, context) };
}
```

The Activity context must be a JNI **global** ref (the `QJniObject` is transient);
it lives for the app's lifetime, so it is never released. `ndk-context` is a
direct Android-only dependency of the backend crate.

## Permissions

Microphone capture uses the existing native permission flow
(`AssetManager.check_microphone_permission()` /
`request_microphone_permission()`, backed by `cpp/android_helpers.h`), not Qt
Multimedia. `android/AndroidManifest.xml` keeps `RECORD_AUDIO`. macOS
`NSMicrophoneUsageDescription` is patched in `CMakeLists.txt`. Removing
`Qt::Multimedia` does not affect permissions.
