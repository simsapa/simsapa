# Android Gradle Plugin — pinned at 8.6.0, do not upgrade opportunistically

`android/build.gradle` pins AGP **8.6.0** and `build-android.sh` pins the JDK to
17–21 (`MAX_JDK_MAJOR=21`). Both are deliberate. Every Android build prints

> WARNING: We recommend using a newer Android Gradle plugin to use compileSdk = 36
> This Android Gradle plugin (8.6.0) was tested up to compileSdk = 35.

**That warning is expected and suppressed** via
`android.suppressUnsupportedCompileSdk=36` in `android/gradle.properties`. Do
not "fix" it by bumping AGP.

**Targeting a newer API level does not require a newer AGP.**
`targetSdkVersion` is just a value written into the manifest; AGP does not gate
it. `compileSdk` is what AGP validates against, and androiddeployqt writes
`androidCompileSdkVersion=android-36` on its own (it picks the newest installed
platform). AGP 8.6.0 accepts that with the warning above and builds fine.

An AGP upgrade touches three coupled things, each of which fails late and
unhelpfully:

1. **The Gradle wrapper version.** AGP 8.10 needs Gradle ≥ 8.11.1; AGP **8.11+
   needs Gradle 8.13**. The wrapper in use is **ours**, checked into
   `android/gradle/wrapper/` at **8.10**, and androiddeployqt copies it into
   `android-build/` with the rest of `android/`. Qt's kits do ship a wrapper —
   at 8.12, in `~/Qt/6.9.3/android_*/src/3rdparty/gradle/gradle/wrapper/` — but
   **that copy is never used** (check the generated
   `android-build/gradle/wrapper/gradle-wrapper.properties`). So the wrapper is
   ours to bump; the pin stands because of reasons 2 and 3.
2. **The JDK pin exists because of AGP's bundled lint.** AGP 8.6.0's lint cannot
   parse a Java 26 version string; `lintVitalAnalyzeRelease` dies with `> 26.0.1`
   as its *entire* error message, **after** all three ABIs have compiled and
   signed. The system default `java` on this machine is 26, which is why
   `build-android.sh` selects a JDK itself instead of inheriting one. Any AGP
   change must re-verify this pin — and the pin should stay regardless.
3. **`android/build.gradle` is a Qt-provided template** using `lintOptions`,
   `aaptOptions` and `packagingOptions` — deprecated through AGP 8.x and
   **removed in AGP 9.x**. Moving to AGP 9 means rewriting a file that has to be
   re-merged on every Qt upgrade.

**Rule: change one variable at a time.** The AGP/Gradle-wrapper bump belongs
with the eventual **Qt upgrade**, when the Qt-provided template and wrapper
change anyway — not with an SDK or targetSdk bump. See
[android-multi-abi-and-chromeos.md](./android-multi-abi-and-chromeos.md)
and `tasks/2026-07-27-131601-prd---android-api-36-compliance-and-packaging-follow-ups.md`.

