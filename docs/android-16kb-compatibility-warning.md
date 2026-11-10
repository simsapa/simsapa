# Android "isn't 16 KB compatible" warning (test-deploy only)

If the Android app shows the dialog **"This app isn't 16 KB compatible"** listing
libraries with "Unknown error", **do not assume the build is broken.** This
warning is gated on the **install path**, not the APK contents. The dialog text
says it outright: *"because this is a debuggable app which is currently being
tested."*

- Installing via **Qt Creator deploy** (`adb install` / `pm install` marks it a
  test deployment) → the dialog appears.
- **Sideloading the byte-identical APK** (copy to phone, tap to install in a
  file manager) → no dialog.

Confirmed empirically: APK sideload → no warning; Qt Creator deploy of the same
build → warning; sideload again → no warning. End users (and any normal
sideload/Play install) never see it.

"Unknown error" next to a library does **not** mean it is misaligned — it means
the on-device checker couldn't verify a library that is stored **compressed**
(`Defl:N`, `extractNativeLibs=true`) in the APK. To prove a given build is fine,
verify the libs directly instead of trusting the dialog:

```sh
# ELF LOAD-segment alignment of a flagged lib (want 0x4000 = 16 KB)
unzip -p app.apk lib/arm64-v8a/libQt6Widgets_arm64-v8a.so > /tmp/x.so
readelf -lW /tmp/x.so | grep LOAD          # last column is p_align
# APK-level 16 KB page alignment
$ANDROID_SDK/build-tools/<ver>/zipalign -c -P 16 4 app.apk && echo PASS
```

(June 2026 forensics: a warning build and a known-good no-warning build were
identical on every 16 KB axis — lib md5s, ELF p_align (Qt libs 0x4000), all 148
libs `Defl:N`, `zipalign -P16` PASS, compileSdk 36 / targetSdk 35 / debuggable.
The only variable was the install path.)

**Separate gap — now resolved:** the app *used to* not be truly 16 KB-compatible
because Qt's 5 bundled FFmpeg prebuilts (`libavcodec`, `libavformat`,
`libavutil`, `libswresample`, `libswscale`) were 4 KB-aligned (0x1000), pulled in
by Qt Multimedia for chanting-practice recording/playback. This was fixed by
replacing Qt Multimedia with a **pure-Rust audio stack** (`cpal` + `flacenc` +
`rubato` + `symphonia`); cpal's Android backend is AAudio (a system lib), so no
audio native library is bundled at all. See
[Pure-Rust audio backend](./pure-rust-audio-backend.md).

**Android NDK — do NOT use r28.** It is incompatible with Qt 6.9.3 at this
project's `minSdkVersion` (libc++ `pthread_cond_clockwait` needs API 30+,
breaking the `cxx` C++ build). **The exclusion is about API 30, not about 27 vs
28** — raising the floor to 28 did not lapse it and does not re-open the
question. Stay on the Qt-supported NDK (r26b/r27); 16 KB alignment of the
main app `.so` is achieved with `target_link_options(... "-Wl,-z,max-page-size=16384")`
in `CMakeLists.txt`, not by relying on r28's default. Details in
[pure-rust-audio-backend.md](./pure-rust-audio-backend.md).

