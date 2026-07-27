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
> `thumbv7neon-linux-androideabi` — that branch is taken only when
> `CMAKE_ANDROID_ARM_MODE` is **false**, and Qt's `android_armv7` toolchain sets
> it true. The real triple is `armv7-linux-androideabi`. Guessing wrong fails
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
# signed App Bundle for Google Play
make android-aab ANDROID_VERSION_CODE=3 ANDROID_VERSION_NAME=1.0.0-alpha.3

# signed APK for sideloading
make android-apk

# unsigned debug APK
make android-apk-debug

# removes the whole build directory (never delete android-build/ by hand)
make android-clean
```

`build-android.sh` also takes `--aab` / `--apk` / `--abis "a;b;c"` / `--debug` /
`--no-sign` / `--clean` directly.

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
properties are unset — which is what Simsapa was shipping. `CMakeLists.txt` now
wires `QT_ANDROID_VERSION_CODE` / `QT_ANDROID_VERSION_NAME` from the
`ANDROID_VERSION_CODE` / `ANDROID_VERSION_NAME` cache variables, forwarded by
`build-android.sh`. **Bump `ANDROID_VERSION_CODE` for every Play upload.**

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
- **`ANDROID_VERSION_CODE` unset** for a signed AAB — a warning, since the build
  would otherwise reuse the CMake cache value (1 on a fresh build directory) and
  Play would reject the upload

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

## 7. Related

- [pure-rust-audio-backend.md](./pure-rust-audio-backend.md) — why the NDK is
  pinned to r26b/r27 (r28 breaks the `cxx` C++ build at minSdk 27) and why 16 KB
  alignment is done with `-Wl,-z,max-page-size=16384` in `CMakeLists.txt`.
- [app-packaging-and-identifiers.md](./app-packaging-and-identifiers.md) — the
  `io.github.simsapa.app` application id vs. the `com.profoundlabs.simsapa` QML
  module URI, which are unrelated and must not be conflated.
- [android-file-saving-saf.md](./android-file-saving-saf.md) — scoped storage,
  which is why `WRITE_EXTERNAL_STORAGE` is not needed.
- `CLAUDE.md` § *Android "isn't 16 KB compatible" warning* — that dialog is
  gated on the install path (Qt Creator deploy vs. sideload), not the contents.
