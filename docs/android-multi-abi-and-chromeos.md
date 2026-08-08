# Android multi-ABI packaging and ChromeOS compatibility

How Simsapa's Android release packages are built: one signed App Bundle
containing native code for **arm64-v8a, x86_64 and armeabi-v7a**, produced by
`make android-aab` → `build-android.sh`, and the manifest rules that keep the
app visible on Chromebooks in the Play Store.

Read this before changing anything under `android/`, the `if (ANDROID)` branch
of `CMakeLists.txt`, or the Qt module list.

---

## 1. What went wrong (July 2026)

A user could not install Simsapa from Google Play on a Chromebook: Play reported
the app as **not compatible**. There were **two independent causes**, and fixing
only one would not have been enough.

### Cause A — the bundle contained only arm64-v8a

Release packages were being built from the Qt Creator interface with the
"Qt 6.9.3 for Android arm64-v8a" kit selected. A Qt Creator kit is single-ABI,
so the AAB contained exactly one:

```sh
$ unzip -l android-build-simsapadhammareader-release.aab | grep '\.so$'
→ base/lib/arm64-v8a/…      # and nothing else
```

Most Chromebooks run Android in ARCVM on **x86_64** hardware. Play filters an
arm64-only bundle off those devices entirely. (ARM Chromebooks — MediaTek,
Qualcomm — would have worked already, which is why this had not been noticed.)

### Cause B — permissions injected by Qt implied *required* hardware features

`android/AndroidManifest.xml` declared only `RECORD_AUDIO`, but it also carried
androiddeployqt's `<!-- %%INSERT_PERMISSIONS -->` marker. androiddeployqt
substitutes that marker with the union of the `<permission>` entries declared in
the `Qt6<Module>_<abi>-android-dependencies.xml` file of every linked Qt module.
For Simsapa's module set that produced a **merged** manifest containing:

| Injected permission | Source |
|---|---|
| `android.permission.CAMERA` | `Qt6WebView_…-android-dependencies.xml` |
| `android.permission.ACCESS_FINE_LOCATION` | `Qt6WebView` / `Qt6Positioning` |
| `android.permission.BLUETOOTH` | `Qt6Bluetooth` |
| `android.permission.WRITE_EXTERNAL_STORAGE` | Qt default set |

Simsapa uses none of them. That would be merely untidy, except that **Google
Play derives required hardware features from permissions**
([App manifest compatibility for Chromebooks](https://developer.android.com/topic/arc/manifest)):

- `CAMERA` implies required `android.hardware.camera` **and**
  `android.hardware.camera.autofocus`
- `ACCESS_FINE_LOCATION` implies required `android.hardware.location.gps`

All three are on Google's "excludes ChromeOS devices" list. So even a
correctly-built multi-ABI bundle would still have been filtered off
Chromebooks.

**The lesson:** the ABI list is the obvious axis and the manifest is the
invisible one. Always check both. `aapt2 dump badging <apk>` prints
`uses-implied-feature:` lines that name the culprit permission explicitly —
that is the fastest way to see this.

---

## 1a. targetSdk 36 (July 2026)

`android/build.gradle`'s `defaultConfig` declares `targetSdkVersion 36`
(`minSdkVersion` stays 27). That one line opts the app in to three behaviours
Android 16 enforces with **no per-app opt-out**:

1. **Edge-to-edge display** — the activity is laid out under the status and
   navigation bars. Clearance comes from Qt's `ApplicationWindow` safe-area
   padding, *not* from any app setting. See
   [android-edge-to-edge-and-safe-areas.md](./android-edge-to-edge-and-safe-areas.md).
2. **Predictive back is on by default** — this **broke back navigation
   outright** (Qt 6.9.3 registers no `OnBackInvokedCallback`, so back closed the
   whole app from every dialog and secondary window). The app opts out with
   `android:enableOnBackInvokedCallback="false"` on the `<activity>`; the
   diagnosis and removal criteria are in the edge-to-edge doc §5.
3. **Orientation and resizability attributes are ignored on large screens**
   (sw ≥ 600 dp), so the app must tolerate arbitrary resize.

`defaultConfig` is the **only** source of the target level: there is no
`<uses-sdk>` in `android/AndroidManifest.xml` and no
`QT_ANDROID_TARGET_SDK_VERSION` in `CMakeLists.txt`.

> **Verify with `aapt2 dump badging`, never by reading the generated
> `gradle.properties`.** androiddeployqt writes `qtTargetSdkVersion=35` into
> `android-build/gradle.properties` and `build.gradle` never reads it — that
> line will still say 35 on a correctly-targeted build and means nothing.
> The same file also carries `qtMinSdkVersion=28`, which our `minSdkVersion 27`
> overrides; see
> [android-qt-upgrade-considerations.md §2.2](./android-qt-upgrade-considerations.md).

`compileSdk` keeps coming from androiddeployqt (`android-36`; it picks the newest
installed platform). AGP 8.6.0 was only tested to 35, so the resulting
"we recommend a newer Android Gradle plugin" warning is suppressed with
`android.suppressUnsupportedCompileSdk=36` in `android/gradle.properties`.
**Targeting a newer API level does not require a newer AGP** — `targetSdkVersion`
is just a value written into the manifest, and AGP does not gate it.

### The debug variant is skipped during release builds

androiddeployqt appends the bare `bundle` task, not `bundleRelease`, so Gradle
built, packaged and signed the **entire debug variant** alongside the release one
— 43 wasted `:*Debug*` tasks. `android/build.gradle` now disables it:

```groovy
androidComponents {
    beforeVariants(selector().withBuildType("debug")) {
        it.enable = !project.hasProperty("simsapaReleaseOnly")
    }
}
```

It must be conditional, because `make android-apk-debug` needs the debug variant
to exist. There is **no `-P` argument to pass** — Gradle is invoked by
androiddeployqt, not by us — so `build-android.sh` delivers the flag through
Gradle's environment mapping instead, exporting
`ORG_GRADLE_PROJECT_simsapaReleaseOnly=true` for release builds only.

> For a debug build the variable must be left **unset**, never set to `false`:
> `project.hasProperty()` is true for *any* value, including `"false"` and the
> empty string.

To confirm it is working, grep the build log for `^> Task .*[Dd]ebug` and **read
the matches, don't count them** — `:stripReleaseDebugSymbols` and
`:mergeReleaseNativeDebugMetadata` are release-variant tasks that merely contain
the word.

---

## 2. The manifest contract

`android/AndroidManifest.xml` now declares permissions **explicitly** and the
`%%INSERT_PERMISSIONS` / `%%INSERT_FEATURES` markers have been **deleted**.

`androiddeployqt`'s `updateFile()` is a plain string replacement over the
manifest (`qtbase/src/tools/androiddeployqt/main.cpp`, ~line 1968). A marker
that is not present is simply a no-op — removing it is safe and is the only way
to get a deterministic permission set.

> **`tools:node="remove"` does not work here.** The Android manifest merger
> applies `tools:node` to nodes contributed by *libraries*, not to nodes in the
> same file. Since androiddeployqt substitutes the injected permissions into
> this very file, a same-file removal is unreliable. Deleting the marker is the
> correct fix.

### Permissions we declare

| Permission | Why |
|---|---|
| `INTERNET` | asset/database downloads, the localhost Rocket API, AI provider requests |
| `ACCESS_NETWORK_STATE` | Qt Network reachability |
| `RECORD_AUDIO` | chanting practice recorder (`cpal`) |
| `MODIFY_AUDIO_SETTINGS` | chanting practice playback |

### Features we mark optional

Every `<uses-feature>` **defaults to `required="true"`**, and Play then filters
out every device lacking it. Simsapa is a text reader; nothing is load-bearing,
so all of these are declared `required="false"`: `touchscreen`, `faketouch`,
`microphone`, `camera`, `camera.autofocus`, `location`, `location.gps`,
`bluetooth`, `telephony`, `wifi`, `sensor.accelerometer`, `sensor.compass`.

`microphone` is optional on purpose: chanting practice needs it, but the rest of
the app does not, and requiring it would exclude mic-less devices.

### Why dropping `WRITE_EXTERNAL_STORAGE` is safe

`minSdkVersion` is 27, and on API 27–28 that permission genuinely is required to
write to shared external storage — so dropping it needed checking rather than
assuming. It is safe here because **Simsapa never touches external storage
directly**: a grep for `getExternalStorage` / `EXTERNAL_STORAGE` / `/sdcard`
across `backend/`, `bridges/`, `cpp/`, `assets/qml/` and `android/` returns
nothing. All user-chosen file I/O goes through the Storage Access Framework
(`content://` tree URIs — see
[android-file-saving-saf.md](./android-file-saving-saf.md)), which needs no
permission, and everything else is written to the app-private data directory.
Re-run that grep if you ever reintroduce a direct filesystem path on Android.

> **⚠️ TRADE-OFF — read this before adding a Qt module.** Because the marker is
> gone, adding a Qt module that needs a runtime permission will **no longer add
> it automatically**. If a feature silently fails on device with a
> permission-denied error, check
> `~/Qt/<ver>/android_<abi>/lib/Qt6<Module>_<abi>-android-dependencies.xml` for
> a `<permission name="…">` entry and add it by hand to
> `android/AndroidManifest.xml`.

---

## 3. How multi-ABI building works in Qt 6

Multi-ABI is **CMake-only**; qmake dropped support in Qt 6.

You configure with the **primary** ABI's `qt-cmake`. During
`qt_add_executable` finalization, `_qt_internal_configure_android_multiabi_target`
(`Qt6AndroidMacros.cmake`) creates one **`ExternalProject` per additional ABI**
that re-runs CMake over the *same source tree* with that ABI's
`qt.toolchain.cmake`. The per-ABI `copy_apk_dependencies` steps are chained into
a serial dependency list — androiddeployqt cannot run in parallel — and only the
primary build produces the final package, merging every ABI's `.so` files into
it.

Two cache variables select the ABIs
([QT_ANDROID_ABIS](https://doc.qt.io/qt-6/cmake-variable-qt-android-abis.html),
[QT_ANDROID_BUILD_ALL_ABIS](https://doc.qt.io/qt-6/cmake-variable-qt-android-build-all-abis.html)):

- `QT_ANDROID_ABIS="arm64-v8a;x86_64;armeabi-v7a"` — explicit list. The primary
  kit's ABI is always included and cannot be excluded.
- `QT_ANDROID_BUILD_ALL_ABIS=ON` — autodetects every installed
  `~/Qt/<ver>/android_*` kit. **Higher priority; it ignores `QT_ANDROID_ABIS`.**

**We use the explicit list, deliberately.** `BUILD_ALL_ABIS` would also pick up
the installed `android_x86` kit, and 32-bit x86 needs the
`i686-linux-android` Rust target, which is not installed and which nothing
requires. Chromebooks run 64-bit ARCVM.

Each requested ABI needs its **Qt for Android kit installed**, or the configure
aborts with *"Cannot find toolchain files for the manually specified Android
ABIs"*. (`QT_PATH_ANDROID_ABI_<abi>` exists for non-standard install layouts;
we don't need it.)

`build-android.sh` forces `-G Ninja`. Qt warns
(`QT_NO_WARN_ANDROID_MULTI_ABI_GENERATOR`) that ExternalProject step ordering is
unreliable with other generators.

### Why the project's per-ABI Qt path logic is safe

`CMakeLists.txt` picks `ANDROID_QT_DIR` (and from it `CMAKE_PREFIX_PATH` and the
`qmake_path` handed to `cxx_qt_import_crate`) by branching on `ANDROID_ABI`,
guarded by `if(NOT CMAKE_PREFIX_PATH)`. That guard is load-bearing and it is
correct, for a non-obvious reason:

**Qt does not forward the parent build's `CMAKE_PREFIX_PATH` to the sub-builds.**
Only `ANDROID_SDK_ROOT`, `ANDROID_NDK_ROOT`, `QT_HOST_PATH`, the build type, a
handful of internal flags, and whatever you name in
`QT_ANDROID_MULTI_ABI_FORWARD_VARS` are passed down. Each sub-build therefore
resolves its own ABI's Qt. Verified:

```
-- Using Android Qt: /home/gambhiro/Qt/6.9.3/android_x86_64    # x86_64 sub-build
-- Using Android Qt: /home/gambhiro/Qt/6.9.3/android_armv7     # armeabi-v7a sub-build
```

If you ever add a variable to `QT_ANDROID_MULTI_ABI_FORWARD_VARS`, make sure it
is genuinely ABI-independent.

---

## 4. Rust targets per ABI

Corrosion's `FindRust.cmake` maps `CMAKE_ANDROID_ARCH_ABI` to a Rust target
triple automatically; nothing in `CMakeLists.txt` sets `Rust_CARGO_TARGET` for
Android, and it should stay that way.

| ABI | Rust target |
|---|---|
| `arm64-v8a` | `aarch64-linux-android` |
| `x86_64` | `x86_64-linux-android` |
| `armeabi-v7a` | **`armv7-linux-androideabi`** |
| `x86` (unused) | `i686-linux-android` |

> **Gotcha.** Reading `FindRust.cmake` suggests `armeabi-v7a` resolves to
> `thumbv7neon-linux-androideabi`. It does not — but **not for the reason the
> code implies**, and the difference matters if anyone ever tries to "fix" it.
> The branch is `if (CMAKE_ANDROID_ARM_MODE)`, and that variable is not a
> boolean. NDK 27 defaults to its **legacy** toolchain file
> (`android.toolchain.cmake` returns straight into
> `android-legacy.toolchain.cmake` unless `ANDROID_USE_LEGACY_TOOLCHAIN_FILE`
> says otherwise), which ends with `set(CMAKE_ANDROID_ARM_MODE
> ${ANDROID_ARM_MODE})` — the literal string **`thumb`** when nothing requests
> ARM mode. CMake's `if()` treats a non-empty, non-false-constant string as
> **true**, so the branch selects `armv7-linux-androideabi` *whatever* the
> instruction mode is; it can never reach the thumb triple under this toolchain.
>
> Measured on 6.10.3 (2026-08-08): a probe configure through the same
> `qt.toolchain.cmake` prints `CMAKE_ANDROID_ARM_MODE='thumb'`, and the
> armeabi-v7a sub-build caches
> `Rust_CARGO_TARGET_CACHED:INTERNAL=armv7-linux-androideabi`. **Neither Qt kit
> sets it** — `ARM_MODE` has zero matches in 6.9.3's *and* 6.10.3's
> `android_armv7/lib/cmake/`, so an earlier version of this note ("Qt's
> `android_armv7` toolchain sets it true") named the wrong source. The C++ is
> therefore compiled thumb while the Rust half uses the ARM triple; ARM/thumb
> interworking makes that benign, and it is what shipped 1.0.0.
>
> The real triple is `armv7-linux-androideabi`. Guessing wrong fails
> late and confusingly, inside the ExternalProject sub-build:
>
> ```
> CMake Error at …/corrosion-src/cmake/Corrosion.cmake:79 (message):
>   Target armv7-linux-androideabi is not installed for toolchain …
> ```
>
> `build-android.sh` pre-flights the whole mapping so the message arrives up
> front, naming the exact `rustup target add` command.

Install once:

```sh
rustup target add aarch64-linux-android x86_64-linux-android armv7-linux-androideabi
```

The ABI sub-builds share one cargo `target/` directory. That is safe — cargo
takes a file lock, so concurrent invocations serialise rather than corrupt each
other. Expect a full multi-ABI build to take roughly 3× a single-ABI build.

**armeabi-v7a is the optional one.** It buys pre-2015 32-bit phones and nothing
else; Chromebooks do not need it. If the 32-bit Rust build ever breaks (tantivy
is the usual suspect on 32-bit, though ARMv7 has `AtomicU64` via `LDREXD`, so
the known [ARMv5TE failure](https://github.com/quickwit-oss/tantivy/issues/743)
does not apply), drop it rather than fighting it:

```sh
make android-aab ANDROID_ABIS='arm64-v8a;x86_64'
```

---

## 5. Building and signing

**Do not build release packages from the Qt Creator interface** — its kits are
single-ABI, which is what caused this whole problem.

```sh
# signed App Bundle for Google Play (no version arguments — see §5)
make android-aab

# signed APK for sideloading
make android-apk

# unsigned debug APK
make android-apk-debug

# the beta package (io.github.simsapa.app.beta) — installs ALONGSIDE the
# released app, which is the only way to test a local build on a device that
# carries the Play install (see the beta doc in §7)
make android-beta-dist          # not debuggable, for GitHub Releases
make android-beta-debug         # debuggable, local only — never distribute
make android-beta-debug-install # adb install -r
make android-beta-debug-run     # launch + stream the log messages

# removes the whole build directory (never delete android-build/ by hand)
make android-clean
```

`build-android.sh` also takes `--aab` / `--apk` / `--abis "a;b;c"` / `--debug` /
`--beta` / `--sign` / `--no-sign` / `--clean` directly.

Switching a build directory between beta and non-beta is safe: the script
records the package identity in `.simsapa-package-identity` and forces a
re-package when it changes. Without that, ninja's `apk` target — which does not
depend on the Gradle property carrying the beta id — is up to date and the
script reports the *previous* build's artifact under the wrong applicationId.

### Signing

Signing goes through androiddeployqt's `--sign`, enabled by the CMake variables
`QT_ANDROID_SIGN_APK` / `QT_ANDROID_SIGN_AAB` and configured entirely through
environment variables:

```
QT_ANDROID_KEYSTORE_PATH
QT_ANDROID_KEYSTORE_ALIAS
QT_ANDROID_KEYSTORE_STORE_PASS
QT_ANDROID_KEYSTORE_KEY_PASS      # optional; script defaults it to the store pass
```

These live in **`android/signing.env`**, which is gitignored. Copy the template
and fill it in once:

```sh
cp android/signing.env.example android/signing.env
```

The upload keystore is `simsapa-upload-keystore.jks`; look up its alias with
`keytool -list -v -keystore <path>`. Any `QT_ANDROID_KEYSTORE_*` already
exported in the shell **overrides** the file, so the passwords can come from a
password manager instead. `.gitignore` also blocks `*.jks` / `*.keystore`
outright.

### The JDK must be 17–21

`android/build.gradle` pins the Android Gradle Plugin to **8.6.0**, which
supports JDK 17–21. On a rolling distro the *system default* `java` is newer
(this machine ships JDK 26), and the failure that produces is one of the worst
error messages in the entire toolchain — the whole build succeeds, all three
ABIs compile and sign, and then the very last Gradle task dies with the JDK's
own version string as the complete explanation:

```
Execution failed for task ':lintVitalAnalyzeRelease'.
> A failure occurred while executing …AndroidLintWorkAction
   > 26.0.1
```

That "26.0.1" is the JDK version: the IntelliJ core bundled inside that AGP's
lint cannot parse a Java 26 version string. Nothing in the message says so.

`build-android.sh` therefore **selects a JDK explicitly** rather than inheriting
whatever `java` resolves to: it honours `ANDROID_JAVA_HOME`, else a `JAVA_HOME`
that is in range, else the highest JDK 17–21 under `/usr/lib/jvm/`. It exports
`JAVA_HOME` (which also gives androiddeployqt its `jarsigner`) and prints the
choice. This is what Qt Creator was doing implicitly with its own configured
JDK, and is why release builds worked there but not from a bare shell.

Raising the AGP version is the real long-term fix; until then the JDK is pinned.

### Cross-ABI contamination: androiddeployqt stages the wrong ABI's plugins

**The multi-ABI packaging pass puts the primary ABI's Qt plugins into the other
ABIs' library folders.** A bundle whose ABI list looked perfect contained **28
aarch64 `.so` files inside `base/lib/x86_64/` and `base/lib/armeabi-v7a/`**,
confirmed by `readelf` (`Machine: AArch64` in the x86_64 directory).

This is **not** staleness. It reproduces exactly from a fully clean tree, and
`androiddeployqt --verbose` shows it happening:

```
  -- Copied …/libs/armeabi-v7a/libplugins_platforms_qtforandroid_arm64-v8a.so
  -- Copied …/libs/armeabi-v7a/libplugins_imageformats_qgif_arm64-v8a.so
```

The same log also contains, from a *different* architecture phase:

```
Skipping "…/android_arm64_v8a/plugins/platforms/libplugins_platforms_qtforandroid_arm64-v8a.so", architecture mismatch
```

— the arm64 plugin paths are present in **every** ABI's resolved dependency
list, and androiddeployqt's own `checkArchitecture` guard rejects them in some
phases but not others. It is an upstream inconsistency, not something the
project configuration causes or can steer.

**The fix is in `android/build.gradle`:** `packagingOptions.jniLibs.excludes`
drops, from each `lib/<abi>/` directory, any library whose Qt ABI-name suffix
names a *different* ABI. Qt suffixes every library it deploys with the ABI name,
so the suffix is reliable evidence; libraries with no ABI suffix
(`libc++_shared.so`, …) match nothing and are kept.

**`build-android.sh` independently re-checks the finished artifact** and
hard-fails on any survivor, so the exclusion list cannot silently stop working
(if Qt renames a plugin scheme, or a fifth ABI appears, the check fires).

Verified after the fix: 139 libraries in each of the three ABI folders, and a
per-folder `readelf` spot check returns AArch64 / ARM / x86-64 respectively.

### Never hand-delete `android-build/`

Deleting the staging directory **wedges the build tree**. The per-ABI copy steps
are ExternalProject steps whose stamps live in the sub-build trees; once those
stamps exist the steps consider themselves up to date and never repopulate the
shared staging directory. Every later build then fails with:

```
Cannot find application binary in build dir …/android-build//libs/armeabi-v7a/libsimsapadhammareader_armeabi-v7a.so.
```

and there is no incremental recovery — the whole build directory must be
reconfigured. Deleting the `*_copy_apk_dependencies_stamp` files does **not**
help; the sub-builds have their own internal stamps.

Use **`make android-clean`** (removes the entire build directory) or
**`make android-rebuild`**.

### Version code

Google Play requires a **strictly increasing `versionCode`** on every upload of
a package. Qt defaults to `versionCode 1` / `versionName "1.0"` when the target
properties are unset — which is what Simsapa was shipping.

The values now flow from two files, so a release needs **no version arguments**:

```
android/version.txt      ─┐
                          ├─ build-android.sh parses & exports ─┐
bridges/Cargo.toml        │   ANDROID_VERSION_CODE / _NAME      │
  [package] version      ─┘                                     │
                                                                v
                              CMakeLists.txt reads $ENV{...} ──> QT_ANDROID_VERSION_*
                                                                     │
                                                                     v
                                              androiddeployqt ──> AndroidManifest.xml
```

**To make a release: edit `android/version.txt`, then `make android-aab`.**
The versionName comes from the `[package]` version in `bridges/Cargo.toml`,
which is bumped every release anyway. A **non-empty** `ANDROID_VERSION_CODE` /
`ANDROID_VERSION_NAME` in the environment still overrides both.

Two things are load-bearing here:

- **The CMake variables are NOT `CACHE`.** A cache entry is written once per
  build directory, so with `CACHE` an edited `version.txt` would be ignored on
  every subsequent build in the same tree. They are plain `$ENV{}` reads, and
  the `QT_ANDROID_VERSION_*` properties are set only when both are non-empty
  (otherwise CMake logs a `STATUS` message and Qt's defaults apply, so a plain
  developer `cmake` configure still succeeds).
- **"Absent" means empty, not unset.** The `Makefile` exports both names
  unconditionally and GNU make exports an *undefined* variable as the **empty
  string**, so every plain `make android-aab` arrives with both set and empty.
  Tests are `[ -n "${VAR:-}" ]` in the shell and
  `if(NOT "${X}" STREQUAL "")` in CMake; an is-set test would read make's empty
  export as a deliberate override and defeat the file parsing.

`build-android.sh` re-runs `cmake -S . -B` on every invocation, which is what
makes an edited `version.txt` take effect. A bare `cmake --build` or a Qt
Creator build reuses the previous configure and keeps the old value.

(The version reaches the package through the primary build only — the ABI
sub-builds just produce `.so` files, so it does not need forwarding.)

---

## 6. Verifying before upload

### What the script checks for you

`build-android.sh` fails fast, **before** the (roughly 3× length) multi-ABI
compile, on:

- a missing Qt for Android kit for any requested ABI
- a missing Rust target for any requested ABI, naming the exact
  `rustup target add` command
- **NDK r28 or newer** — the default NDK resolution picks the highest installed
  version, which would silently select an NDK that breaks the `cxx` build at
  minSdk 27
- **no JDK 17–21** — see below; the native failure mode is a Gradle lint crash
  whose entire error message is the JDK version number
- **`jarsigner` missing** from the selected JDK — androiddeployqt's `signAAB()`
  uses jarsigner (from the JDK, *not* the Android SDK) and only looks for it at
  the very end of the build
- missing keystore credentials
- **an unreadable or invalid `android/version.txt`** — a hard failure before the
  CMake configure, naming the file. The versionCode must be a positive integer;
  the file is *parsed*, never sourced. Likewise an unparseable `[package]`
  version in `bridges/Cargo.toml`

Afterwards it prints the artifact path and mtime and the ABIs present, then runs
two audits:

**Cross-ABI contamination** (hard failure) — every packaged library's Qt ABI-name
suffix must match its `lib/<abi>/` directory. See "Release builds must start
clean" above for what this catches and why.

**ChromeOS compatibility** (warning) — lists the permissions that survived the
merge, flags the ones from which Play infers required hardware (CAMERA,
ACCESS_\*_LOCATION, the telephony set), and flags any `<uses-feature>` lacking
`android:required="false"`.

The ChromeOS audit reads the **merged** manifest
(`build/intermediates/merged_manifests/<variant>/…/AndroidManifest.xml`), not
the source one — the whole point is to catch what a Qt module or an AndroidX
dependency contributed behind our back. It is a genuine regression test: run
against the pre-fix arm64-only build it reproduces exactly the two findings that
caused the incident.

It **parses the XML** rather than grepping. Grepping was tried and was not good
enough: the explanatory XML *comment* in `android/AndroidManifest.xml` that
mentions `<uses-feature>` is carried into the merged manifest verbatim and
matched the pattern, producing a false positive that then tripped `set -e`.
ElementTree ignores comments and normalises attribute wrapping. The audit is
advisory and never changes the exit status.

(`aapt2 dump badging` would be the more familiar tool, but it only works on
APKs; the merged manifest covers AAB builds too.)

### By hand

```sh
# ABIs actually present
unzip -l <artifact>.aab | grep -oE '(base/)?lib/[a-z0-9_-]+/' | sort -u

# the Chromebook trap, APK only: any line here WITHOUT required='false' is a problem
$ANDROID_SDK_ROOT/build-tools/<ver>/aapt2 dump badging <artifact>.apk \
    | grep -E "uses-feature|uses-implied-feature"

# 16 KB page alignment (see the CLAUDE.md Android section)
$ANDROID_SDK_ROOT/build-tools/<ver>/zipalign -c -P 16 4 <artifact>.apk
```

`aapt2` and `zipalign` are not on `PATH`; they live in
`$ANDROID_SDK_ROOT/build-tools/<version>/`.

After upload, **Play Console → Release → App bundle explorer → Device
catalog**, filtered by form factor "Chromebook", states the per-device exclusion
reason directly. Use it instead of guessing.

## 6a. Build warnings that are harmless by construction

Every Android build prints seven `QML import could not be resolved` warnings.
All were investigated in July 2026 and none indicate a problem.

- **`com.profoundlabs.simsapa`** — the app's *own* module. It is **not** shipped
  as a plugin directory: there is no `assets/android_rcc_bundle/` in the package
  at all, and the module is compiled into the app binary as Qt resources
  (`strings` on `libsimsapadhammareader_arm64-v8a.so` shows
  `:/qt/qml/com/profoundlabs/simsapa/…` paths and the matching `<qresource
  prefix=…>` header), put there by `cxx_qt_import_qml_module` in
  `bridges/build.rs`. androiddeployqt's import scanner walks **on-disk** import
  paths, so a compiled-in module is unresolvable by construction. Expect this
  warning to persist permanently.
- **`QtWebEngine`** — imported only by `SuttaHtmlView_Desktop.qml` and
  `DictionaryHtmlView_Desktop.qml`. Qt WebEngine has no Android port and those
  desktop-only views are never instantiated there (Android uses QtWebView). The
  scanner reads every QML file regardless of platform.
- **`QtQuick.Controls.Windows` / `.macOS` / `.iOS`** — other platforms' control
  styles, referenced by QtQuick.Controls' own module metadata and absent from the
  Android kit.
- **`QtWayland.Compositor`, `QtQuick3D.MaterialEditor`** — not imported by any app
  QML; transitive references from Qt's own modules.

### Closed decisions (July 2026) — do not re-litigate without new evidence

- **Keep `armeabi-v7a`.** There are users on 32-bit ARM phones. The cost is ~1/3
  of the multi-ABI build time and an extra slice in the bundle that **no user
  downloads** — Play delivers one ABI per device.
- **Do not add the 32-bit `x86` ABI.** Unused; Chromebook ARCVM is 64-bit; it
  would need the `i686-linux-android` Rust target, which is not installed. This
  is also why `build-android.sh` deliberately avoids `QT_ANDROID_BUILD_ALL_ABIS`
  — it would autodetect the installed `android_x86` Qt kit and fail.
- **Do not remove `package=` from `AndroidManifest.xml`**, despite Gradle
  recommending it on every build. It is androiddeployqt's actual source of truth
  for the application id (`extractPackageName()` rejects the literal token
  `androidPackageName` from `build.gradle`'s `namespace` line and falls back to
  this attribute), and changing it would change the application id of a
  published app. The reasoning is also a comment in the manifest itself.
- **Bundle size needs no action.** The 2026-07-28 AAB is 284 MB, but per-device
  delivery is ~62 MB compressed (one ABI + dex + resources), against Play's
  200 MB limit. The 105 MB `BUNDLE-METADATA` entry is native debug symbols:
  Play strips it from delivery and uses it to symbolicate native crashes, so it
  is worth keeping.

### R8 / ProGuard stays **off** — the "no deobfuscation file" warning is expected

Every upload to the Play Console raises:

> There is no deobfuscation file associated with this App Bundle. If you use
> obfuscated code (R8/proguard), uploading a deobfuscation file will make crashes
> and ANRs easier to analyse and debug. Using R8/proguard can help reduce app
> size.

**This warning is expected and is not acted on.** It is informational and never
blocks a release. It refers *only* to R8's `mapping.txt` for **Java/Kotlin
bytecode** — it has nothing to do with native code, and the app is Rust + C++ +
QML.

There is no `minifyEnabled` line in `android/build.gradle` and no
`proguard-rules.pro` anywhere in the tree. That is deliberate, for three reasons.

**1. The size argument does not apply to this app.** Uncompressed content of the
release bundle (2026-07-29 build):

| | uncompressed |
|---|---|
| native `.so` (3 ABIs) | 521.6 MB |
| assets / resources / other | 356.1 MB |
| **`base/dex/classes.dex`** | **4.25 MB** |

Dex is ~0.5% of the bundle; `libsimsapadhammareader_arm64-v8a.so` alone is
108 MB. R8 might remove a megabyte or two of dex — against a bundle whose
per-device delivery is ~62 MB compressed, well under Play's 200 MB limit
(see *Bundle size needs no action* above). There is no meaningful size win
available in the Java layer.

**2. The breakage risk is real, and recurring.** Almost all of that dex is
**Qt's own Java**, not ours — `Qt6Android.jar`, `Qt6AndroidQuick.jar`,
`QtAndroidWebView.jar` and friends. Qt's Android port is driven end to end by
JNI reflection: `QtNative`, `QtLoader`, `QtActivityDelegate` method lookups, and
the activity/service classes named **as strings** in `AndroidManifest.xml`.
Shrinking and obfuscation are exactly what breaks reflective lookup.

Qt 6.9.3 ships **no ProGuard keep-rules file** — verified, there is no
`*proguard*` file anywhere in the Android kits. So enabling R8 means authoring
the keep set by hand against Qt internals, with:

- a failure mode of `ClassNotFoundException` / `NoSuchMethodError` **at runtime,
  in release builds only** — i.e. after a three-ABI compile-and-sign, and
  plausibly not until a specific screen is opened on a device;
- re-validation required on **every Qt upgrade**, since the keep set tracks Qt's
  internal class and method names.

This is also the same one-variable-at-a-time discipline that keeps the AGP and
JDK pins where they are: minification is a release-only code transform, and it
does not belong next to an SDK bump.

**3. The half of the warning that actually matters is already satisfied.**
Crashes in this app land in native code, not Java — and the bundle **already
ships native debug symbols**. AGP's `extractReleaseNativeSymbolTables` runs
implicitly (nothing sets `debugSymbolLevel` in `android/build.gradle`,
`build-android.sh` or `CMakeLists.txt`), producing:

```
BUNDLE-METADATA/com.android.tools.build.debugsymbols/arm64-v8a/libsimsapadhammareader_arm64-v8a.so.sym   129.9 MB
                                            .../x86_64/…                                                 127.9 MB
                                            .../armeabi-v7a/…                                            110.6 MB
```

(~368 MB uncompressed; the ~105 MB `BUNDLE-METADATA` entry noted under *Bundle
size needs no action*.) Play strips these from delivery and uses them to
symbolicate native stack traces. The app `.so` is built unstripped —
`file` reports `with debug_info, not stripped` — which is what makes the symbol
tables extractable.

**Verify once per release cycle** that the Play Console's Crashes & ANRs page
shows *symbolicated* native frames. Play has historically capped the native
debug symbols payload, so if that ever regresses, the fix is to reduce symbol
coverage (`debugSymbolLevel 'SYMBOL_TABLE'` on fewer ABIs, dropping x86_64
first as the least-used slice) — **not** to enable R8, which would not help
native symbolication at all.

**Do not re-litigate without new evidence.** New evidence would be: the Java
layer growing to a size where dex is a material fraction of the download, or Qt
shipping official ProGuard keep-rules for its Android port.

### `useLegacyPackaging` stays `true`

`packagingOptions.jniLibs.useLegacyPackaging true` (`android/build.gradle:59`,
a **Qt-provided template line**, not ours) writes
`android:extractNativeLibs="true"` into the merged manifest. It controls exactly
one thing: how the ~139 `.so` files per ABI are stored in the package and how the
dynamic loader reaches them.

| | `true` (current, "legacy") | `false` (AGP's default for new projects since 4.2) |
|---|---|---|
| storage in the APK | **compressed** (`Defl:N`) | **uncompressed**, page-aligned |
| at install time | extracted to `/data/app/…/lib/<abi>/` | nothing extracted |
| at load time | `dlopen` of a real file on disk | `mmap`ed straight out of the APK zip |
| copies on device | two (retained APK + extracted `lib/`) | one |
| minimum API | any | 23 (we are at 27, so not a constraint) |

**What flipping it to `false` would buy.**

- **On-device footprint.** This is the only argument with real weight. Legacy
  keeps the libraries twice — compressed inside the retained APK *and*
  uncompressed in the extracted `lib/` directory. `false` keeps one copy. With
  ~139 Qt/Rust libraries per ABI the saving is roughly the size of the extracted
  `lib/` directory.
- **Faster installs, smaller delta updates.** No extraction pass, and
  uncompressed libraries diff far better between versions than deflated ones, so
  Play's incremental update patches shrink.
- **The 16 KB checker could actually verify the libraries.** The
  "This app isn't 16 KB compatible … Unknown error" dialog (see `AGENTS.md`
  § *Android "isn't 16 KB compatible" warning*) lists libraries **because** they
  are stored compressed and the on-device checker cannot inspect them.
  Uncompressed + aligned would let it read the real `p_align`. Cosmetic — the
  dialog is already known to be gated on the install path, not the contents —
  but it would stop being a recurring question.

**What it would cost.**

- **A bigger APK/AAB**, since the libraries are no longer deflated. Play
  re-compresses for delivery so the *download* penalty is usually small — but
  "usually" is unmeasured here, and that is precisely the number that would have
  to be taken.
- **It changes the on-device layout of every native library** in an app with an
  unusually large native surface: Qt platform and QML plugins, QtWebView,
  tantivy, the pure-Rust `cpal` audio stack, the cxx-qt bridge. Anything that
  resolves a library by filesystem path rather than by name breaks. Qt 6 loads by
  name and should be unaffected — but "should be" has not been tested on this
  app, on three ABIs.
- **`android/build.gradle` is a Qt-provided template** (the same fact that keeps
  AGP pinned at 8.6.0). Diverging from it adds another line to re-merge on every
  Qt upgrade.
- **No constraint is currently being violated**, so there is no problem to fix —
  see the numbers below.

**Conclusion: keep `true`.** The 2026-07-28 build already satisfies every
constraint that matters: per-device download is ~62 MB against Play's 200 MB
limit, `zipalign -c -P 16` passes, and **all** 139 arm64-v8a and 139 x86_64
libraries carry `p_align=0x4000`. The only benefit on offer is install
footprint, which nobody has reported as a problem, weighed against changing the
loading path of every native library in the app. That is not a change worth
making next to a targetSdk bump — it violates the project's own **change one
variable at a time** rule.

**It has deliberately not been measured both ways.** The decision above is made
on the reasoning, not on numbers: measuring costs a second full multi-ABI build
plus an on-device install, and would not change the answer while no constraint is
tight. Recorded so the omission is not mistaken for an oversight.

**Revisit it with the Qt upgrade**, when `build.gradle` has to be re-merged
anyway and the native stack is being re-validated regardless. At that point take
the measurements both ways: AAB/APK size, on-device install footprint (`du` of
the installed app), `zipalign -c -P 16`, and `readelf -lW` `p_align` for the app
`.so` and a Qt library. Flip it only if the footprint saving is material *and*
all three ABIs still load.

---

## 7. Related

- [android-edge-to-edge-and-safe-areas.md](./android-edge-to-edge-and-safe-areas.md)
  — targetSdk 36's edge-to-edge enforcement, safe-area padding, the
  predictive-back opt-out and the deprecated bar-colour APIs in Play's report.
- [android-qt-upgrade-considerations.md](./android-qt-upgrade-considerations.md)
  — work deferred to the Qt upgrade: removing the predictive-back opt-out,
  raising `minSdkVersion` to 28, and the AGP / Gradle-wrapper / JDK coupling.
- [pure-rust-audio-backend.md](./pure-rust-audio-backend.md) — why the NDK is
  pinned to r26b/r27 (r28 breaks the `cxx` C++ build at minSdk 27) and why 16 KB
  alignment is done with `-Wl,-z,max-page-size=16384` in `CMakeLists.txt`.
- [app-packaging-and-identifiers.md](./app-packaging-and-identifiers.md) — the
  `io.github.simsapa.app` application id vs. the `com.profoundlabs.simsapa` QML
  module URI, which are unrelated and must not be conflated.
- [android-beta-distribution-and-play-policy.md](./android-beta-distribution-and-play-policy.md)
  — the `io.github.simsapa.app.beta` package and why it exists (a Play install
  is signed by Play App Signing and can never be replaced by a local build), the
  `make android-beta-*` targets, reading log messages with `adb logcat` instead
  of deploying from Qt Creator, and the in-app update notice's Play-policy
  gating.
- [android-file-saving-saf.md](./android-file-saving-saf.md) — scoped storage,
  which is why `WRITE_EXTERNAL_STORAGE` is not needed.
- `CLAUDE.md` § *Android "isn't 16 KB compatible" warning* — that dialog is
  gated on the install path (Qt Creator deploy vs. sideload), not the contents.
