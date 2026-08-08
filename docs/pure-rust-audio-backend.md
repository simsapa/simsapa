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

**The exclusion is not tied to minSdk 27, and does not lapse at 28.** Bionic
gained `pthread_cond_clockwait` only at API 30, so raising `minSdkVersion` to 28
(done as part of the Qt 6.10.3 upgrade) changes nothing here. Raising it to 30 to
satisfy r28 would drop Android 8–10 devices, so that is not the fix either.

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
